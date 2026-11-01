//! Windows implementation of the chrome compositing operations.
//!
//! wry gives every webview its own container HWND parented to the window, and
//! places each new one at the top of the z-order. Tauri exposes no way to
//! reorder them, but it does hand out the underlying `ICoreWebView2Controller`,
//! whose `ParentWindow` is exactly that container. From there both operations
//! are ordinary Win32.
//!
//! Page observation goes through the same controller, to the `ICoreWebView2`
//! events Tauri does not surface: `SourceChanged` fires for every URL change,
//! including the ones a single-page app makes without loading a document.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant};

use tauri::Manager;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2BrowserExtension, ICoreWebView2CallDevToolsProtocolMethodCompletedHandler,
    ICoreWebView2Controller, ICoreWebView2Deferral, ICoreWebView2Environment13, ICoreWebView2FrameInfo,
    ICoreWebView2FrameInfo2, ICoreWebView2NavigationStartingEventArgs3, ICoreWebView2NewWindowRequestedEventArgs,
    ICoreWebView2ProcessExtendedInfoCollection, ICoreWebView2Profile7, ICoreWebView2ScriptDialogOpeningEventArgs,
    ICoreWebView2_13, ICoreWebView2_19, ICoreWebView2_2, ICoreWebView2_20, ICoreWebView2_3, ICoreWebView2_8,
    COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_JPEG, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL,
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    COREWEBVIEW2_NAVIGATION_KIND, COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD, COREWEBVIEW2_PROCESS_KIND,
    COREWEBVIEW2_PROCESS_KIND_BROWSER, COREWEBVIEW2_PROCESS_KIND_GPU, COREWEBVIEW2_PROCESS_KIND_RENDERER,
    COREWEBVIEW2_PROCESS_KIND_UTILITY, COREWEBVIEW2_SCRIPT_DIALOG_KIND, COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD,
    COREWEBVIEW2_SCRIPT_DIALOG_KIND_CONFIRM, COREWEBVIEW2_SCRIPT_DIALOG_KIND_PROMPT, COREWEBVIEW2_WEB_ERROR_STATUS,
    COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED,
};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2ContextMenuItemCollection, ICoreWebView2ContextMenuRequestedEventArgs, ICoreWebView2_11,
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_CHECK_BOX,
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_RADIO, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU, COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND,
    COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_AUDIO, COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_IMAGE,
    COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_SELECTED_TEXT, COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_VIDEO,
};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2DownloadOperation, ICoreWebView2_4, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_DOWNLOAD_PROCESS_CRASHED,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_ACCESS_DENIED,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_BLOCKED_BY_POLICY, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_FAILED,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_HASH_MISMATCH, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_MALICIOUS,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NAME_TOO_LONG, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NO_SPACE,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_SECURITY_CHECK_FAILED,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_TOO_LARGE, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_TOO_SHORT,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_TRANSIENT_ERROR,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_DISCONNECTED, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_FAILED,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_INVALID_REQUEST,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_SERVER_DOWN, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_TIMEOUT,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_BAD_CONTENT,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_CERTIFICATE_PROBLEM,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_CONTENT_LENGTH_MISMATCH,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_CROSS_ORIGIN_REDIRECT,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_FAILED, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_FORBIDDEN,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_NO_RANGE, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_UNAUTHORIZED,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_UNEXPECTED_RESPONSE,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_PAUSED,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_SHUTDOWN, COREWEBVIEW2_DOWNLOAD_STATE,
    COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED, COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS,
};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2ProcessFailedEventArgs2, COREWEBVIEW2_PROCESS_FAILED_KIND,
    COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED, COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_REASON, COREWEBVIEW2_PROCESS_FAILED_REASON_OUT_OF_MEMORY,
    COREWEBVIEW2_PROCESS_FAILED_REASON_TERMINATED,
};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
    COREWEBVIEW2_PHYSICAL_KEY_STATUS,
};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
};
use webview2_com::{
    take_pwstr, AcceleratorKeyPressedEventHandler, BrowserExtensionEnableCompletedHandler,
    BrowserExtensionRemoveCompletedHandler, BytesReceivedChangedEventHandler,
    CallDevToolsProtocolMethodCompletedHandler, CapturePreviewCompletedHandler,
    ContainsFullScreenElementChangedEventHandler, ContextMenuRequestedEventHandler, DOMContentLoadedEventHandler,
    DevToolsProtocolEventReceivedEventHandler, DocumentTitleChangedEventHandler, DownloadStartingEventHandler,
    ExecuteScriptCompletedHandler, FocusChangedEventHandler, GetProcessExtendedInfosCompletedHandler,
    IsDocumentPlayingAudioChangedEventHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler, ProcessFailedEventHandler,
    ProfileAddBrowserExtensionCompletedHandler, ProfileGetBrowserExtensionsCompletedHandler,
    ScriptDialogOpeningEventHandler, SourceChangedEventHandler, StateChangedEventHandler, TrySuspendCompletedHandler,
    WindowCloseRequestedEventHandler,
};
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, E_POINTER, HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HRGN, RGN_DIFF, RGN_OR,
};
use windows::Win32::System::Com::{IStream, STREAM_SEEK_END, STREAM_SEEK_SET};
use windows::Win32::System::ProcessStatus::{
    GetPerformanceInfo, GetProcessMemoryInfo, PERFORMANCE_INFORMATION, PROCESS_MEMORY_COUNTERS,
    PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, VIRTUAL_KEY, VK_0, VK_9, VK_A, VK_CONTROL, VK_ESCAPE, VK_F1, VK_F24, VK_HOME,
    VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR, VK_RIGHT, VK_SHIFT, VK_TAB, VK_Z,
};
use windows::Win32::UI::Shell::{SHCreateMemStream, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNA,
    SW_SHOWNORMAL,
};

use super::memory::{EngineSnapshot, ProcessKind, ProcessSample};
use super::workers::{self, WorkerTracker, IDLE_GRACE};
use super::{
    DownloadAction, DownloadSignal, DownloadSink, DownloadStart, DownloadStartSink, FailureSink, KeySink, PageSignal,
    PageSink, PhysicalRect, ProcessFailure, RoundedRect,
};
use crate::browser::BLANK_URL;
use crate::error::{HakuError, Result};
use crate::model::{
    allowed_items, offers, plain_label, ContextMenuRequestId, ContextMenuTarget, ContextMenuTargetKind, NativeMenuItem,
    NativeMenuItemKind, PageMenu,
};
use crate::model::{
    shortcut_for, Chord, Commit, CrashCause, DialogAnswer, DialogId, DialogKind, DownloadFailure, DownloadId,
    DownloadState, Key, MemoryStatus, NavigationKind, PageDialog, Placement, Settled, SlotId, WindowRequestId,
};
use crate::webview::inject;

/// How long to wait for the main thread to run a native operation.
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Source of dialog ids, unique for the life of the process.
static NEXT_DIALOG: AtomicU64 = AtomicU64::new(1);

/// Source of new-window request ids, unique for the life of the process.
static NEXT_WINDOW: AtomicU64 = AtomicU64::new(1);

/// Source of context menu request ids, unique for the life of the process.
static NEXT_MENU: AtomicU64 = AtomicU64::new(1);

type PendingDialog = (
    ICoreWebView2ScriptDialogOpeningEventArgs,
    ICoreWebView2Deferral,
    Option<SlotId>,
);
type PendingWindow = (
    ICoreWebView2NewWindowRequestedEventArgs,
    ICoreWebView2Deferral,
    Option<SlotId>,
);

thread_local! {
    /// Dialogs pages are paused on, awaiting an answer.
    ///
    /// Thread-local because the WebView2 objects involved belong to the UI
    /// thread: they are stored by the handler that runs there and taken back by
    /// the answer, which is dispatched there too.
    static PENDING_DIALOGS: RefCell<HashMap<DialogId, PendingDialog>> =
        RefCell::new(HashMap::new());

    /// Pages waiting on a new window, held like dialogs, and for the same
    /// reason on this thread.
    static PENDING_WINDOWS: RefCell<HashMap<WindowRequestId, PendingWindow>> =
        RefCell::new(HashMap::new());

    /// Pages waiting on the context menu Haku draws for them, with the items
    /// they offered, held like dialogs, and for the same reason on this thread.
    static PENDING_MENUS: RefCell<HashMap<ContextMenuRequestId, PendingMenu>> = RefCell::new(HashMap::new());

    /// The chrome webview's DevTools session and what it has seen of the
    /// profile's service workers. UI-thread only, like the webview itself.
    static WORKERS: RefCell<HashMap<bool, WorkerWatch>> = RefCell::new(HashMap::new());

    /// Extensions this process installed, by id, as the engine handed them
    /// over, to switch on and off. UI-thread only, like the engine's objects.
    static EXTENSIONS: RefCell<HashMap<String, ICoreWebView2BrowserExtension>> = RefCell::new(HashMap::new());

    /// Downloads that can still be paused, resumed or cancelled, with the
    /// slot each runs in. UI-thread only, like the engine's objects.
    static DOWNLOADS: RefCell<HashMap<DownloadId, Followed>> = RefCell::new(HashMap::new());

    /// Failed downloads Haku resumed, with their address and slot, until
    /// they run again. The engine resumes such a download as a new one,
    /// into a new file, and leaves the old operation behind. UI-thread only.
    static RESUMED: RefCell<Vec<(DownloadId, String, Option<SlotId>)>> = const { RefCell::new(Vec::new()) };
}

/// A download Haku can still act on.
struct Followed {
    /// What the download runs as now.
    operation: ICoreWebView2DownloadOperation,
    slot: Option<SlotId>,
    /// What it ran as before Haku resumed it after a failure. Kept alive:
    /// once the last of these is released, the engine stops reporting the
    /// operation that replaced it, though the file still grows.
    replaced: Vec<ICoreWebView2DownloadOperation>,
}

/// Whether `operation` is the one a listed download runs as now, rather than
/// one the engine left behind when it resumed the download as a new one.
fn is_current(id: DownloadId, operation: &ICoreWebView2DownloadOperation) -> bool {
    DOWNLOADS.with(|downloads| {
        downloads
            .borrow()
            .get(&id)
            .is_some_and(|followed| followed.operation == *operation)
    })
}

/// Where downloads are reported, set once by [`observe_downloads`].
static DOWNLOAD_SINKS: OnceLock<(DownloadStartSink, DownloadSink)> = OnceLock::new();

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

/// Runs `action` against a webview's `ICoreWebView2` on the UI thread, without
/// waiting for it: nothing that freezing or resuming does is worth blocking on.
fn with_core<R, F>(webview: &tauri::Webview<R>, action: F) -> Result<()>
where
    R: tauri::Runtime,
    F: FnOnce(&ICoreWebView2) -> windows::core::Result<()> + Send + 'static,
{
    webview
        .with_webview(move |platform| {
            let _ = unsafe { platform.controller().CoreWebView2() }.and_then(|core| action(&core));
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))
}

/// Asks the engine to trim a webview's memory, or to stop trimming it. Older
/// runtimes lack the setting, and freezing still works without it.
unsafe fn set_memory_target(core: &ICoreWebView2, level: COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL) {
    if let Ok(core) = core.cast::<ICoreWebView2_19>() {
        let _ = core.SetMemoryUsageTargetLevel(level);
    }
}

pub fn freeze<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    with_core(webview, |core| unsafe {
        set_memory_target(core, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW);
        // Declining is not an error worth reporting: the page simply keeps
        // running, as it would have with freezing turned off.
        let done = TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(())));
        core.cast::<ICoreWebView2_3>()?.TrySuspend(&done)
    })
}

pub fn resume<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    with_core(webview, |core| unsafe {
        set_memory_target(core, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL);
        core.cast::<ICoreWebView2_3>()?.Resume()
    })
}

pub fn evaluate<R: tauri::Runtime>(webview: &tauri::Webview<R>, expression: &str, timeout: Duration) -> Option<String> {
    let (sender, receiver) = mpsc::channel();
    let expression = HSTRING::from(expression);
    webview
        .with_webview(move |platform| {
            let handler = ExecuteScriptCompletedHandler::create(Box::new(move |status, result| {
                let _ = sender.send(status.ok().map(|()| result));
                Ok(())
            }));
            let _ = unsafe {
                platform
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.ExecuteScript(&expression, &handler))
            };
        })
        .ok()?;
    receiver.recv_timeout(timeout).ok().flatten()
}

pub fn leave_page<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    expression: &str,
    capture: bool,
    timeout: Duration,
) -> (Option<String>, Option<Vec<u8>>) {
    let deadline = Instant::now() + timeout;
    let (state_sender, state_receiver) = mpsc::channel();
    let (image_sender, image_receiver) = mpsc::channel();
    let expression = HSTRING::from(expression);
    let dispatched = webview.with_webview(move |platform| {
        let Ok(core) = (unsafe { platform.controller().CoreWebView2() }) else {
            return;
        };
        let read = ExecuteScriptCompletedHandler::create(Box::new(move |status, result| {
            let _ = state_sender.send(status.ok().map(|()| result));
            Ok(())
        }));
        let _ = unsafe { core.ExecuteScript(&expression, &read) };

        if !capture {
            return;
        }
        let Some(stream) = (unsafe { SHCreateMemStream(None) }) else {
            return;
        };
        let target = stream.clone();
        let captured = CapturePreviewCompletedHandler::create(Box::new(move |status| {
            let image = status.ok().and_then(|()| read_stream(&target));
            let _ = image_sender.send(image);
            Ok(())
        }));
        let _ = unsafe { core.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_JPEG, &stream, &captured) };
    });
    if dispatched.is_err() {
        return (None, None);
    }

    let remaining = || deadline.saturating_duration_since(Instant::now());
    let state = state_receiver.recv_timeout(remaining()).ok().flatten();
    // A sender dropped without sending, as when no capture was asked for,
    // ends this wait at once.
    let image = image_receiver.recv_timeout(remaining()).ok().flatten();
    (state, image)
}

/// The whole contents of an in-memory stream.
fn read_stream(stream: &IStream) -> Option<Vec<u8>> {
    let mut size = 0_u64;
    unsafe { stream.Seek(0, STREAM_SEEK_END, Some(&mut size)) }.ok()?;
    unsafe { stream.Seek(0, STREAM_SEEK_SET, None) }.ok()?;
    let length = u32::try_from(size).ok()?;
    let mut bytes = vec![0_u8; length as usize];
    let mut read = 0_u32;
    unsafe { stream.Read(bytes.as_mut_ptr().cast(), length, Some(&mut read)) }
        .ok()
        .ok()?;
    bytes.truncate(read as usize);
    Some(bytes)
}

pub fn memory_status() -> Option<MemoryStatus> {
    let mut info = PERFORMANCE_INFORMATION {
        cb: size_of::<PERFORMANCE_INFORMATION>() as u32,
        ..Default::default()
    };
    unsafe { GetPerformanceInfo(&mut info, info.cb) }.ok()?;
    // Every figure is in pages.
    let bytes = |pages: usize| pages as u64 * info.PageSize as u64;
    Some(MemoryStatus {
        total_physical: bytes(info.PhysicalTotal),
        available_physical: bytes(info.PhysicalAvailable),
        commit_limit: bytes(info.CommitLimit),
        commit_total: bytes(info.CommitTotal),
    })
}

pub fn engine_processes<R: tauri::Runtime>(
    chrome: &tauri::Webview<R>,
    slots: &[(SlotId, tauri::Webview<R>)],
) -> Result<EngineSnapshot> {
    let frames = slots
        .iter()
        .map(|(slot, webview)| (*slot, main_frame_id(webview)))
        .collect();

    let (sender, receiver) = mpsc::channel();
    chrome
        .with_webview(move |platform| {
            let requested = unsafe { request_process_infos(&platform.controller(), sender.clone()) };
            if let Err(error) = requested {
                let _ = sender.send(Err(error));
            }
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;
    let listed = receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    // Read here rather than in the completion handler, which runs on the UI
    // thread.
    let processes = listed
        .into_iter()
        .map(|(pid, kind, main_frames)| ProcessSample {
            pid,
            kind,
            private_bytes: private_bytes(pid).unwrap_or(0),
            main_frames,
        })
        .collect();
    Ok(EngineSnapshot { frames, processes })
}

/// The id of a webview's main frame, as the engine lists it per process.
/// Nothing when the runtime predates it or the webview cannot be reached.
fn main_frame_id<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Option<u32> {
    let (sender, receiver) = mpsc::channel();
    webview
        .with_webview(move |platform| {
            let id = unsafe {
                platform
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.cast::<ICoreWebView2_20>())
                    .and_then(|core| {
                        let mut id = 0u32;
                        core.FrameId(&mut id).map(|()| id)
                    })
            };
            let _ = sender.send(id.ok());
        })
        .ok()?;
    receiver.recv_timeout(DISPATCH_TIMEOUT).ok().flatten()
}

/// A process id, its kind, and the main frames of the frames it hosts.
type ListedProcess = (u32, ProcessKind, Vec<u32>);

unsafe fn request_process_infos(
    controller: &ICoreWebView2Controller,
    sender: mpsc::Sender<windows::core::Result<Vec<ListedProcess>>>,
) -> windows::core::Result<()> {
    let environment = controller
        .CoreWebView2()?
        .cast::<ICoreWebView2_2>()?
        .Environment()?
        .cast::<ICoreWebView2Environment13>()?;
    let handler = GetProcessExtendedInfosCompletedHandler::create(Box::new(move |status, infos| {
        let listed = status.and_then(|()| match infos {
            Some(infos) => read_process_infos(&infos),
            None => Ok(Vec::new()),
        });
        let _ = sender.send(listed);
        Ok(())
    }));
    environment.GetProcessExtendedInfos(&handler)
}

unsafe fn read_process_infos(
    infos: &ICoreWebView2ProcessExtendedInfoCollection,
) -> windows::core::Result<Vec<ListedProcess>> {
    let mut count = 0u32;
    infos.Count(&mut count)?;
    let mut listed = Vec::with_capacity(count as usize);
    for index in 0..count {
        let info = infos.GetValueAtIndex(index)?;
        let process = info.ProcessInfo()?;
        let mut pid = 0i32;
        process.ProcessId(&mut pid)?;
        let mut kind = COREWEBVIEW2_PROCESS_KIND::default();
        process.Kind(&mut kind)?;

        let mut main_frames = Vec::new();
        // The iterator does not keep its collection alive: iterating after the
        // collection is released crashes the process.
        let collection = info.AssociatedFrameInfos()?;
        let frames = collection.GetIterator()?;
        let mut has_current = BOOL::default();
        frames.HasCurrent(&mut has_current)?;
        while has_current.as_bool() {
            if let Some(id) = main_frame_of(&frames.GetCurrent()?) {
                if !main_frames.contains(&id) {
                    main_frames.push(id);
                }
            }
            frames.MoveNext(&mut has_current)?;
        }
        listed.push((pid as u32, process_kind(kind), main_frames));
    }
    Ok(listed)
}

/// Deeper than any page nests frames in practice.
const MAX_FRAME_DEPTH: usize = 64;

/// Follows a frame up to the main frame of its page.
unsafe fn main_frame_of(frame: &ICoreWebView2FrameInfo) -> Option<u32> {
    let mut frame = frame.cast::<ICoreWebView2FrameInfo2>().ok()?;
    // The main frame's parent reads as an error. Bounded so a malformed chain
    // cannot spin.
    for _ in 0..MAX_FRAME_DEPTH {
        match frame
            .ParentFrameInfo()
            .and_then(|parent| parent.cast::<ICoreWebView2FrameInfo2>())
        {
            Ok(parent) => frame = parent,
            Err(_) => {
                let mut id = 0u32;
                return frame.FrameId(&mut id).ok().map(|()| id);
            }
        }
    }
    None
}

fn process_kind(kind: COREWEBVIEW2_PROCESS_KIND) -> ProcessKind {
    match kind {
        COREWEBVIEW2_PROCESS_KIND_BROWSER => ProcessKind::Browser,
        COREWEBVIEW2_PROCESS_KIND_RENDERER => ProcessKind::Renderer,
        COREWEBVIEW2_PROCESS_KIND_GPU => ProcessKind::Gpu,
        COREWEBVIEW2_PROCESS_KIND_UTILITY => ProcessKind::Utility,
        _ => ProcessKind::Other,
    }
}

/// A process's commit charge, in bytes.
fn private_bytes(pid: u32) -> Option<u64> {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let read = GetProcessMemoryInfo(
            process,
            (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters.cb,
        );
        let _ = CloseHandle(process);
        read.ok()?;
    }
    Some(counters.PrivateUsage as u64)
}

pub fn raise_chrome<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    with_hwnd(webview, |hwnd| unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .map_err(|error| HakuError::WindowMissing(error.to_string()))
    })
}

pub fn set_visible<R: tauri::Runtime>(webview: &tauri::Webview<R>, visible: bool) -> Result<()> {
    let (sender, receiver) = mpsc::channel();
    webview
        .with_webview(move |platform| {
            let outcome = unsafe {
                let controller = platform.controller();
                let mut hwnd = HWND::default();
                controller.ParentWindow(&mut hwnd).and_then(|()| {
                    // Wry's show uses SW_SHOW, which may activate the container.
                    let _ = ShowWindow(hwnd, if visible { SW_SHOWNA } else { SW_HIDE });
                    controller.SetIsVisible(visible)
                })
            }
            .map_err(|error| HakuError::WindowMissing(error.to_string()));
            let _ = sender.send(outcome);
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;
    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

/// A region covering `rect`, with its corners rounded to `radius` when above zero.
///
/// The caller owns the region and must delete it.
unsafe fn rounded_region(rect: PhysicalRect, radius: i32) -> HRGN {
    if radius > 0 {
        // CreateRoundRectRgn takes the full width and height of the ellipse.
        CreateRoundRectRgn(rect.x, rect.y, rect.right(), rect.bottom(), radius * 2, radius * 2)
    } else {
        CreateRectRgn(rect.x, rect.y, rect.right(), rect.bottom())
    }
}

pub fn set_input_mask<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    viewports: &[RoundedRect],
    overlays: &[RoundedRect],
) -> Result<()> {
    let overlays = overlays.to_vec();
    let viewports = viewports.to_vec();

    with_hwnd(webview, move |hwnd| unsafe {
        // No viewport means nothing should show through: drop the region so the
        // chrome is a whole window again.
        if viewports.is_empty() {
            SetWindowRgn(hwnd, None, true);
            return Ok(());
        }

        let mut client = Default::default();
        GetClientRect(hwnd, &mut client).map_err(|error| HakuError::WindowMissing(error.to_string()))?;

        let region = CreateRectRgn(0, 0, client.right, client.bottom);
        // A rounded hole is what gives the page rounded corners: the chrome
        // keeps painting the corner, and the page shows through the curve.
        for pane in &viewports {
            let hole = rounded_region(pane.rect, pane.radius);
            CombineRgn(Some(region), Some(region), Some(hole), RGN_DIFF);
            let _ = DeleteObject(hole.into());
        }

        // A rounded overlay is added back rounded for the same reason the other
        // way round: a square patch would paint chrome over the page at its
        // corners.
        for overlay in &overlays {
            let patch = rounded_region(overlay.rect, overlay.radius);
            CombineRgn(Some(region), Some(region), Some(patch), RGN_OR);
            let _ = DeleteObject(patch.into());
        }

        // The window owns the region after this call; it must not be deleted.
        if SetWindowRgn(hwnd, Some(region), true) == 0 {
            let _ = DeleteObject(region.into());
            return Err(HakuError::WindowMissing("SetWindowRgn failed".into()));
        }
        Ok(())
    })
}

pub fn show_fatal(message: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from("Haku"),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn watch_failures<R: tauri::Runtime>(webview: &tauri::Webview<R>, sink: FailureSink) -> Result<()> {
    let (sender, receiver) = mpsc::channel();

    webview
        .with_webview(move |platform| {
            let outcome = unsafe { platform.controller().CoreWebView2() }
                .and_then(|core| unsafe { attach_failure_handler(&core, sink) })
                .map_err(|error| HakuError::WindowMissing(error.to_string()));
            let _ = sender.send(outcome);
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

pub fn allow_clipboard_read<R: tauri::Runtime>(chrome: &tauri::Webview<R>) -> Result<()> {
    let (sender, receiver) = mpsc::channel();

    chrome
        .with_webview(move |platform| {
            let outcome = unsafe { platform.controller().CoreWebView2() }
                .and_then(|core| unsafe { attach_clipboard_permission(&core) })
                .map_err(|error| HakuError::WindowMissing(error.to_string()));
            let _ = sender.send(outcome);
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

/// Allows clipboard reads, and leaves every other permission to the engine.
unsafe fn attach_clipboard_permission(core: &ICoreWebView2) -> windows::core::Result<()> {
    let handler = PermissionRequestedEventHandler::create(Box::new(|_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
        args.PermissionKind(&mut kind)?;
        if kind == COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ {
            args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
        }
        Ok(())
    }));
    let mut token = 0i64;
    core.add_PermissionRequested(&handler, &mut token)
}

unsafe fn attach_failure_handler(core: &ICoreWebView2, sink: FailureSink) -> windows::core::Result<()> {
    let handler = ProcessFailedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
        args.ProcessFailedKind(&mut kind)?;
        // Older runtimes give no reason, and the failure is then of no known cause.
        let reason = args
            .cast::<ICoreWebView2ProcessFailedEventArgs2>()
            .ok()
            .and_then(|args| {
                let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON::default();
                args.Reason(&mut reason).ok().map(|()| reason)
            });
        if let Some(failure) = failure_of(kind, reason) {
            sink(failure);
        }
        Ok(())
    }));
    let mut token = 0i64;
    core.add_ProcessFailed(&handler, &mut token)
}

/// The failure Haku answers for a kind of process exiting, if any.
fn failure_of(
    kind: COREWEBVIEW2_PROCESS_FAILED_KIND,
    reason: Option<COREWEBVIEW2_PROCESS_FAILED_REASON>,
) -> Option<ProcessFailure> {
    match kind {
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => Some(ProcessFailure::Page(match reason {
            Some(COREWEBVIEW2_PROCESS_FAILED_REASON_OUT_OF_MEMORY) => CrashCause::OutOfMemory,
            Some(COREWEBVIEW2_PROCESS_FAILED_REASON_TERMINATED) => CrashCause::Ended,
            _ => CrashCause::Unknown,
        })),
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => Some(ProcessFailure::Engine),
        _ => None,
    }
}

pub fn intercept_keys<R: tauri::Runtime>(webview: &tauri::Webview<R>, sink: KeySink) -> Result<()> {
    let (sender, receiver) = mpsc::channel();

    webview
        .with_webview(move |platform| {
            let outcome = unsafe { attach_key_handler(&platform.controller(), sink) }
                .map_err(|error| HakuError::WindowMissing(error.to_string()));
            let _ = sender.send(outcome);
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

unsafe fn attach_key_handler(controller: &ICoreWebView2Controller, sink: KeySink) -> windows::core::Result<()> {
    let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
        args.KeyEventKind(&mut kind)?;
        // Alt combinations arrive as system keys.
        if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
            return Ok(());
        }
        let mut code = 0u32;
        args.VirtualKey(&mut code)?;
        let Some(key) = key_of(VIRTUAL_KEY(code as u16)) else {
            return Ok(());
        };
        let held = |key: VIRTUAL_KEY| GetKeyState(i32::from(key.0)) < 0;
        let chord = Chord {
            key,
            ctrl: held(VK_CONTROL),
            shift: held(VK_SHIFT),
            alt: held(VK_MENU),
        };
        let Some(shortcut) = shortcut_for(chord) else {
            return Ok(());
        };
        // Kept from the page and from the engine's own handling of the key.
        args.SetHandled(true)?;
        // A held key repeats. Acting on every repeat would open a tab per
        // repeat of Ctrl+T, so only the first press counts.
        let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
        args.PhysicalKeyStatus(&mut status)?;
        if !status.WasKeyDown.as_bool() {
            sink(shortcut);
        }
        Ok(())
    }));
    let mut token = 0i64;
    controller.add_AcceleratorKeyPressed(&handler, &mut token)
}

/// The keymap's name for a virtual key, for the keys shortcuts use.
fn key_of(code: VIRTUAL_KEY) -> Option<Key> {
    let key = match code {
        code if (VK_A.0..=VK_Z.0).contains(&code.0) => Key::Letter(char::from(code.0 as u8)),
        code if (VK_0.0..=VK_9.0).contains(&code.0) => Key::Digit((code.0 - VK_0.0) as u8),
        code if (VK_F1.0..=VK_F24.0).contains(&code.0) => Key::Function((code.0 - VK_F1.0 + 1) as u8),
        VK_TAB => Key::Tab,
        VK_ESCAPE => Key::Escape,
        VK_HOME => Key::Home,
        VK_LEFT => Key::Left,
        VK_RIGHT => Key::Right,
        VK_PRIOR => Key::PageUp,
        VK_NEXT => Key::PageDown,
        _ => return None,
    };
    Some(key)
}

pub fn observe_page<R: tauri::Runtime>(webview: &tauri::Webview<R>, sink: PageSink) -> Result<()> {
    let slot = SlotId::from_label(webview.label());
    let (sender, receiver) = mpsc::channel();

    webview
        .with_webview(move |platform| {
            let outcome = unsafe { attach_observers(&platform.controller(), &sink, slot) }
                .map_err(|error| HakuError::WindowMissing(error.to_string()));
            let _ = sender.send(outcome);
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

pub fn observe_popup<R: tauri::Runtime>(webview: &tauri::Webview<R>, sink: PageSink) -> Result<()> {
    let (sender, receiver) = mpsc::channel();

    webview
        .with_webview(move |platform| {
            let outcome = unsafe { platform.controller().CoreWebView2() }
                .and_then(|core| unsafe { attach_window_requests(&core, &sink, None) })
                .map_err(|error| HakuError::WindowMissing(error.to_string()));
            let _ = sender.send(outcome);
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

pub fn engine_window<R: tauri::Runtime>(webview: &tauri::Webview<R>, timeout: Duration) -> Option<String> {
    let (sender, receiver) = mpsc::channel();
    webview
        .with_webview(move |platform| {
            let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |status, result| {
                let _ = sender.send(status.ok().map(|()| result));
                Ok(())
            }));
            let _ = unsafe {
                platform.controller().CoreWebView2().and_then(|core| {
                    core.CallDevToolsProtocolMethod(
                        &HSTRING::from("Browser.getWindowForTarget"),
                        &HSTRING::from("{}"),
                        &handler,
                    )
                })
            };
        })
        .ok()?;
    receiver.recv_timeout(timeout).ok().flatten()
}

unsafe fn attach_observers(
    controller: &ICoreWebView2Controller,
    sink: &PageSink,
    slot: Option<SlotId>,
) -> windows::core::Result<()> {
    let core = controller.CoreWebView2()?;
    let keys = sink.clone();
    attach_key_handler(
        controller,
        Arc::new(move |shortcut| keys(PageSignal::Shortcut(shortcut))),
    )?;
    let failures = sink.clone();
    attach_failure_handler(
        &core,
        Arc::new(move |failure| failures(PageSignal::ProcessFailed(failure))),
    )?;
    // Handlers stay registered for the webview's whole life, so the tokens that
    // would unregister them are never needed.
    let mut token = 0i64;

    let on_focus = {
        let sink = sink.clone();
        FocusChangedEventHandler::create(Box::new(move |_, _| {
            sink(PageSignal::Focused);
            Ok(())
        }))
    };
    controller.add_GotFocus(&on_focus, &mut token)?;

    let on_starting = {
        let sink = sink.clone();
        NavigationStartingEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            // Older runtimes lack the navigation kind; they keep native behaviour.
            let Ok(args) = args.cast::<ICoreWebView2NavigationStartingEventArgs3>() else {
                return Ok(());
            };

            let mut kind = COREWEBVIEW2_NAVIGATION_KIND::default();
            args.NavigationKind(&mut kind)?;
            // Haku never traverses a webview's native history itself, so any
            // back or forward here came from the page, a key or a mouse button.
            if kind == COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD {
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                args.SetCancel(true)?;
                sink(PageSignal::TraverseRequested { url: take_pwstr(uri) });
                return Ok(());
            }
            // Only a navigation with a body carries a content type, and a
            // top-level one with a body is a form submission.
            let mut form = BOOL::default();
            args.RequestHeaders()?
                .Contains(&HSTRING::from("Content-Type"), &mut form)?;
            sink(PageSignal::NavigationStarted { form: form.as_bool() });
            Ok(())
        }))
    };
    core.add_NavigationStarting(&on_starting, &mut token)?;

    let on_source = {
        let sink = sink.clone();
        SourceChangedEventHandler::create(Box::new(move |sender, args| {
            let (Some(core), Some(args)) = (sender, args) else {
                return Ok(());
            };
            let mut new_document = BOOL::default();
            args.IsNewDocument(&mut new_document)?;
            drain(&core, sink.clone(), Some(new_document.as_bool()))
        }))
    };
    core.add_SourceChanged(&on_source, &mut token)?;

    // A title change is drained through the same log as a URL change, so both
    // reach the browser in the order the page made them. A single-page app
    // pushes its route and then retitles, and the title belongs to the new entry.
    let on_title = {
        let sink = sink.clone();
        DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(core) = sender else { return Ok(()) };
            drain(&core, sink.clone(), None)
        }))
    };
    core.add_DocumentTitleChanged(&on_title, &mut token)?;

    // A reloading tab's scroll and draft go back in once its document exists.
    let on_loaded = {
        let sink = sink.clone();
        DOMContentLoadedEventHandler::create(Box::new(move |sender, _| {
            let Some(core) = sender else { return Ok(()) };
            let mut source = PWSTR::null();
            core.Source(&mut source)?;
            sink(PageSignal::Loaded {
                url: take_pwstr(source),
            });
            Ok(())
        }))
    };
    core.cast::<ICoreWebView2_2>()?
        .add_DOMContentLoaded(&on_loaded, &mut token)?;

    // The page has laid out and run its `load` handlers, which is when a
    // reloading tab's scroll goes back in and its cover can come off.
    let on_completed = {
        let sink = sink.clone();
        NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
            let (Some(core), Some(args)) = (sender, args) else {
                return Ok(());
            };
            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
            args.WebErrorStatus(&mut status)?;
            if status == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED {
                return Ok(());
            }
            let mut source = PWSTR::null();
            core.Source(&mut source)?;
            sink(PageSignal::Completed {
                url: take_pwstr(source),
            });
            Ok(())
        }))
    };
    core.add_NavigationCompleted(&on_completed, &mut token)?;

    // The webview's own dialogs are replaced by Haku's, which match the
    // interface and name the site asking. The page stays paused on a deferral
    // until the interface answers.
    core.Settings()?.SetAreDefaultScriptDialogsEnabled(false)?;
    let on_dialog = {
        let sink = sink.clone();
        ScriptDialogOpeningEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let dialog = read_dialog(&args)?;
            let deferral = args.GetDeferral()?;
            PENDING_DIALOGS.with(|pending| pending.borrow_mut().insert(dialog.id, (args, deferral, slot)));
            sink(PageSignal::DialogRequested(dialog));
            Ok(())
        }))
    };
    core.add_ScriptDialogOpening(&on_dialog, &mut token)?;

    attach_window_requests(&core, sink, slot)?;
    attach_context_menu(&core, sink, slot)?;
    attach_downloads(&core, slot)?;

    // Whether a page is playing audio decides whether a smart policy leaves it
    // running in the background. Older runtimes lack the event, and a page is
    // then treated as silent.
    if let Ok(core) = core.cast::<ICoreWebView2_8>() {
        let on_audio = {
            let sink = sink.clone();
            IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(core) = sender.and_then(|sender| sender.cast::<ICoreWebView2_8>().ok()) else {
                    return Ok(());
                };
                let mut playing = BOOL::default();
                core.IsDocumentPlayingAudio(&mut playing)?;
                sink(PageSignal::AudioChanged {
                    playing: playing.as_bool(),
                });
                Ok(())
            }))
        };
        core.add_IsDocumentPlayingAudioChanged(&on_audio, &mut token)?;
    }

    // A page entering fullscreen takes its window with it. The engine only
    // makes the element fill the webview; the window is Haku's to change.
    let on_fullscreen = {
        let sink = sink.clone();
        ContainsFullScreenElementChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(core) = sender else { return Ok(()) };
            let mut fullscreen = BOOL::default();
            core.ContainsFullScreenElement(&mut fullscreen)?;
            sink(PageSignal::FullscreenChanged {
                fullscreen: fullscreen.as_bool(),
            });
            Ok(())
        }))
    };
    core.add_ContainsFullScreenElementChanged(&on_fullscreen, &mut token)?;
    Ok(())
}

/// Reports a webview's requests for a new window and to close its own, the
/// part of [`attach_observers`] an extension's toolbar popup needs too.
unsafe fn attach_window_requests(
    core: &ICoreWebView2,
    sink: &PageSink,
    slot: Option<SlotId>,
) -> windows::core::Result<()> {
    let mut token = 0i64;
    // Haku opens every window itself, as a tab or a window of its own. wry
    // has already marked the request handled, which on its own opens nothing;
    // the page waits on a deferral until a webview is handed to it or it is
    // refused.
    let on_window = {
        let sink = sink.clone();
        NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            args.Uri(&mut uri)?;
            let mut gesture = BOOL::default();
            args.IsUserInitiated(&mut gesture)?;
            let features = args.WindowFeatures()?;
            let (mut sized, mut placed) = (BOOL::default(), BOOL::default());
            features.HasSize(&mut sized)?;
            features.HasPosition(&mut placed)?;
            let (mut left, mut top, mut width, mut height) = (0u32, 0u32, 0u32, 0u32);
            if placed.as_bool() {
                features.Left(&mut left)?;
                features.Top(&mut top)?;
            }
            if sized.as_bool() {
                features.Width(&mut width)?;
                features.Height(&mut height)?;
            }
            // The engine hands a position left of or above the main screen,
            // which is negative, wrapped into its unsigned type.
            let placement = Placement {
                position: placed
                    .as_bool()
                    .then(|| (f64::from(left.cast_signed()), f64::from(top.cast_signed()))),
                size: sized.as_bool().then(|| (f64::from(width), f64::from(height))),
            };
            // The physical keys: the click happened in the engine's process,
            // so this thread's own key state may not have seen it.
            let held = |key: VIRTUAL_KEY| GetAsyncKeyState(i32::from(key.0)) < 0;
            args.SetHandled(true)?;
            let request = WindowRequestId(NEXT_WINDOW.fetch_add(1, Ordering::Relaxed));
            let deferral = args.GetDeferral()?;
            PENDING_WINDOWS.with(|pending| pending.borrow_mut().insert(request, (args, deferral, slot)));
            sink(PageSignal::WindowRequested {
                request,
                url: take_pwstr(uri),
                popup: sized.as_bool() || placed.as_bool(),
                placement,
                // Ctrl+Shift opens a link in the foreground, as in Chrome.
                background: held(VK_CONTROL) && !held(VK_SHIFT),
                window: held(VK_SHIFT) && !held(VK_CONTROL),
                gesture: gesture.as_bool(),
            });
            Ok(())
        }))
    };
    core.add_NewWindowRequested(&on_window, &mut token)?;

    let on_close = {
        let sink = sink.clone();
        WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
            sink(PageSignal::CloseRequested);
            Ok(())
        }))
    };
    core.add_WindowCloseRequested(&on_close, &mut token)?;
    Ok(())
}

pub fn observe_downloads(started: DownloadStartSink, changed: DownloadSink) -> Result<()> {
    DOWNLOAD_SINKS
        .set((started, changed))
        .map_err(|_| HakuError::Unsupported("downloads are already observed".into()))
}

/// Takes over the downloads a webview starts: the engine's own download
/// interface never shows, the file goes where the engine would put it, and
/// what happens to it is reported to [`DOWNLOAD_SINKS`]. Older runtimes lack
/// the event, and keep the engine's interface.
unsafe fn attach_downloads(core: &ICoreWebView2, slot: Option<SlotId>) -> windows::core::Result<()> {
    let Ok(core) = core.cast::<ICoreWebView2_4>() else {
        return Ok(());
    };
    let mut token = 0i64;
    let on_download = DownloadStartingEventHandler::create(Box::new(move |_, args| {
        let (Some(args), Some((started, changed))) = (args, DOWNLOAD_SINKS.get()) else {
            return Ok(());
        };
        args.SetHandled(true)?;
        let operation = args.DownloadOperation()?;
        let mut uri = PWSTR::null();
        operation.Uri(&mut uri)?;
        let mut path = PWSTR::null();
        args.ResultFilePath(&mut path)?;
        let (url, path) = (take_pwstr(uri), take_pwstr(path));
        let (_, total) = read_bytes(&operation)?;
        let resumed = RESUMED.with(|resumed| {
            let mut resumed = resumed.borrow_mut();
            let at = resumed
                .iter()
                .position(|(_, address, owner)| *address == url && *owner == slot)?;
            Some(resumed.remove(at).0)
        });
        let id = match resumed {
            Some(id) => {
                changed(id, DownloadSignal::Restarted { path, total });
                id
            }
            None => started(slot, DownloadStart { url, path, total }),
        };

        let mut token = 0i64;
        let on_bytes = BytesReceivedChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(operation) = sender else { return Ok(()) };
            if !is_current(id, &operation) {
                return Ok(());
            }
            // Bytes arriving mean it runs on as the same download.
            RESUMED.with(|resumed| resumed.borrow_mut().retain(|(resumed, ..)| *resumed != id));
            let (received, total) = read_bytes(&operation)?;
            changed(id, DownloadSignal::Progress { received, total });
            Ok(())
        }));
        operation.add_BytesReceivedChanged(&on_bytes, &mut token)?;
        let on_state = StateChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(operation) = sender else { return Ok(()) };
            if !is_current(id, &operation) {
                return Ok(());
            }
            let settled = read_settled(&operation)?;
            // Nothing more can be done to one that ended for good.
            if !settled.state.is_active() && !settled.can_resume {
                DOWNLOADS.with(|downloads| downloads.borrow_mut().remove(&id));
            }
            changed(id, DownloadSignal::Settled(settled));
            Ok(())
        }));
        operation.add_StateChanged(&on_state, &mut token)?;
        DOWNLOADS.with(|downloads| {
            let mut downloads = downloads.borrow_mut();
            let replaced = downloads
                .remove(&id)
                .map(|earlier| {
                    let mut replaced = earlier.replaced;
                    replaced.push(earlier.operation);
                    replaced
                })
                .unwrap_or_default();
            downloads.insert(
                id,
                Followed {
                    operation,
                    slot,
                    replaced,
                },
            );
        });
        Ok(())
    }));
    core.add_DownloadStarting(&on_download, &mut token)?;
    Ok(())
}

/// Bytes received, and the file's size when the server said.
unsafe fn read_bytes(operation: &ICoreWebView2DownloadOperation) -> windows::core::Result<(u64, Option<u64>)> {
    let (mut received, mut total) = (0i64, 0i64);
    operation.BytesReceived(&mut received)?;
    operation.TotalBytesToReceive(&mut total)?;
    // An unknown size reads as -1.
    Ok((received.max(0).cast_unsigned(), u64::try_from(total).ok()))
}

unsafe fn read_settled(operation: &ICoreWebView2DownloadOperation) -> windows::core::Result<Settled> {
    let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
    operation.State(&mut state)?;
    let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
    operation.InterruptReason(&mut reason)?;
    let mut can_resume = BOOL::default();
    operation.CanResume(&mut can_resume)?;
    let (received, total) = read_bytes(operation)?;
    let (state, failure) = match state {
        COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS => (DownloadState::InProgress, None),
        COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED => (DownloadState::Completed, None),
        _ => interruption(reason),
    };
    Ok(Settled {
        state,
        failure,
        received,
        total,
        can_resume: can_resume.as_bool(),
    })
}

/// What an interrupted download's reason means to Haku.
fn interruption(reason: COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON) -> (DownloadState, Option<DownloadFailure>) {
    let failure = match reason {
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_PAUSED => return (DownloadState::Paused, None),
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_SHUTDOWN => {
            return (DownloadState::Cancelled, None)
        }
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_FAILED
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_TIMEOUT
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_DISCONNECTED
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_SERVER_DOWN
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_INVALID_REQUEST => DownloadFailure::Network,
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_DOWNLOAD_PROCESS_CRASHED => DownloadFailure::Other,
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_FAILED
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_ACCESS_DENIED
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NO_SPACE
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NAME_TOO_LONG
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_TOO_LARGE
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_MALICIOUS
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_TRANSIENT_ERROR
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_BLOCKED_BY_POLICY
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_SECURITY_CHECK_FAILED
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_TOO_SHORT
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_HASH_MISMATCH => DownloadFailure::File,
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_FAILED
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_NO_RANGE
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_BAD_CONTENT
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_UNAUTHORIZED
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_CERTIFICATE_PROBLEM
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_FORBIDDEN
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_UNEXPECTED_RESPONSE
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_CONTENT_LENGTH_MISMATCH
        | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_CROSS_ORIGIN_REDIRECT => DownloadFailure::Server,
        _ => DownloadFailure::Other,
    };
    (DownloadState::Failed, Some(failure))
}

pub fn control_download<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    id: DownloadId,
    action: DownloadAction,
) -> Result<bool> {
    let (sender, receiver) = mpsc::channel();
    app.run_on_main_thread(move || {
        let listed = DOWNLOADS.with(|downloads| {
            downloads
                .borrow()
                .get(&id)
                .map(|followed| (followed.operation.clone(), followed.slot))
        });
        let acted = listed.map(|(operation, slot)| unsafe {
            if action == DownloadAction::Resume
                && read_settled(&operation).is_ok_and(|settled| settled.state == DownloadState::Failed)
            {
                let mut uri = PWSTR::null();
                if operation.Uri(&mut uri).is_ok() {
                    RESUMED.with(|resumed| resumed.borrow_mut().push((id, take_pwstr(uri), slot)));
                }
            }
            let result = match action {
                DownloadAction::Pause => operation.Pause(),
                DownloadAction::Resume => operation.Resume(),
                DownloadAction::Cancel => operation.Cancel(),
            };
            if result.is_err() {
                // No restart follows a resume the engine refused.
                RESUMED.with(|resumed| resumed.borrow_mut().retain(|(resumed, ..)| *resumed != id));
            }
            result.map_err(|error| error.message())
        });
        let _ = sender.send(acted);
    })?;
    match receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
    {
        None => Ok(false),
        Some(Ok(())) => Ok(true),
        Some(Err(message)) => Err(HakuError::Unsupported(format!("download {}: {message}", id.0))),
    }
}

pub fn open_file(path: &Path) -> Result<()> {
    let opened = unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(path.as_os_str()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // Anything above 32 is success; the rest are error codes.
    if opened.0 as isize > 32 {
        Ok(())
    } else {
        Err(HakuError::Unsupported(format!("opening {}", path.display())))
    }
}

pub fn show_file(path: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    // Explorer reads `/select,` and the quoted path as one argument; a
    // Windows path never holds a quote.
    std::process::Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .spawn()
        .map(drop)
        .map_err(|error| HakuError::Unsupported(format!("showing {}: {error}", path.display())))
}

/// A page's context menu waiting on an answer, and what it offered.
struct PendingMenu {
    args: ICoreWebView2ContextMenuRequestedEventArgs,
    deferral: ICoreWebView2Deferral,
    items: Vec<NativeMenuItem>,
    slot: Option<SlotId>,
}

/// Replaces the engine's context menu with the one Haku draws: the request is
/// marked handled, so the engine shows nothing, and waits on a deferral until
/// [`answer_context_menu`]. Older runtimes lack the event, and keep the
/// engine's menu.
unsafe fn attach_context_menu(
    core: &ICoreWebView2,
    sink: &PageSink,
    slot: Option<SlotId>,
) -> windows::core::Result<()> {
    let Ok(core) = core.cast::<ICoreWebView2_11>() else {
        return Ok(());
    };
    let mut token = 0i64;
    let on_menu = {
        let sink = sink.clone();
        ContextMenuRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let menu = read_menu(&args)?;
            args.SetHandled(true)?;
            let deferral = args.GetDeferral()?;
            // One menu is open at a time: a newer request means the older one
            // was dismissed, whether or not its answer has arrived.
            let stale: Vec<PendingMenu> =
                PENDING_MENUS.with(|pending| pending.borrow_mut().drain().map(|(_, menu)| menu).collect());
            for menu in stale {
                let _ = menu.deferral.Complete();
            }
            PENDING_MENUS.with(|pending| {
                pending.borrow_mut().insert(
                    menu.request,
                    PendingMenu {
                        args,
                        deferral,
                        items: menu.items.clone(),
                        slot,
                    },
                )
            });
            sink(PageSignal::ContextMenuRequested(menu));
            Ok(())
        }))
    };
    core.add_ContextMenuRequested(&on_menu, &mut token)?;
    Ok(())
}

unsafe fn read_menu(args: &ICoreWebView2ContextMenuRequestedEventArgs) -> windows::core::Result<PageMenu> {
    let mut location = POINT::default();
    args.Location(&mut location)?;
    let target = args.ContextMenuTarget()?;

    let mut kind = COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND::default();
    target.Kind(&mut kind)?;
    let kind = match kind {
        COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_IMAGE => ContextMenuTargetKind::Image,
        COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_SELECTED_TEXT => ContextMenuTargetKind::Selection,
        COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_AUDIO => ContextMenuTargetKind::Audio,
        COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_VIDEO => ContextMenuTargetKind::Video,
        _ => ContextMenuTargetKind::Page,
    };
    let flag = |read: &dyn Fn(*mut BOOL) -> windows::core::Result<()>| -> windows::core::Result<bool> {
        let mut value = BOOL::default();
        read(&mut value)?;
        Ok(value.as_bool())
    };
    let text = |read: &dyn Fn(*mut PWSTR) -> windows::core::Result<()>| -> windows::core::Result<String> {
        let mut value = PWSTR::null();
        read(&mut value)?;
        Ok(take_pwstr(value))
    };
    let optional = |has: bool, read: &dyn Fn(*mut PWSTR) -> windows::core::Result<()>| {
        if has {
            text(read).map(Some)
        } else {
            Ok(None)
        }
    };

    let has_link = flag(&|v| target.HasLinkUri(v))?;
    let has_link_text = flag(&|v| target.HasLinkText(v))?;
    let has_source = flag(&|v| target.HasSourceUri(v))?;
    let has_selection = flag(&|v| target.HasSelection(v))?;
    let target = ContextMenuTarget {
        kind,
        page_url: text(&|v| target.PageUri(v))?,
        link_url: optional(has_link, &|v| target.LinkUri(v))?,
        link_text: optional(has_link_text, &|v| target.LinkText(v))?,
        source_url: optional(has_source, &|v| target.SourceUri(v))?,
        selection: optional(has_selection, &|v| target.SelectionText(v))?,
        editable: flag(&|v| target.IsEditable(v))?,
    };

    let items = read_items(&args.MenuItems()?)?;
    #[cfg(debug_assertions)]
    log_item_names(&items, 0);
    Ok(PageMenu {
        request: ContextMenuRequestId(NEXT_MENU.fetch_add(1, Ordering::Relaxed)),
        x: location.x,
        y: location.y,
        target,
        items: allowed_items(items),
    })
}

/// Every item the engine offered, before any is dropped.
unsafe fn read_items(
    collection: &ICoreWebView2ContextMenuItemCollection,
) -> windows::core::Result<Vec<NativeMenuItem>> {
    let mut count = 0u32;
    collection.Count(&mut count)?;
    let mut items = Vec::with_capacity(count as usize);
    for index in 0..count {
        let item = collection.GetValueAtIndex(index)?;
        let mut kind = COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND::default();
        item.Kind(&mut kind)?;
        let kind = match kind {
            COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_CHECK_BOX => NativeMenuItemKind::Checkbox,
            COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_RADIO => NativeMenuItemKind::Radio,
            COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR => NativeMenuItemKind::Separator,
            COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU => NativeMenuItemKind::Submenu,
            _ => NativeMenuItemKind::Command,
        };
        let mut id = 0i32;
        item.CommandId(&mut id)?;
        let (mut name, mut label, mut shortcut) = (PWSTR::null(), PWSTR::null(), PWSTR::null());
        item.Name(&mut name)?;
        item.Label(&mut label)?;
        item.ShortcutKeyDescription(&mut shortcut)?;
        let (mut enabled, mut checked) = (BOOL::default(), BOOL::default());
        item.IsEnabled(&mut enabled)?;
        item.IsChecked(&mut checked)?;
        let children = match kind {
            NativeMenuItemKind::Submenu => read_items(&item.Children()?)?,
            _ => Vec::new(),
        };
        let shortcut = take_pwstr(shortcut);
        items.push(NativeMenuItem {
            id,
            name: take_pwstr(name),
            label: plain_label(&take_pwstr(label)),
            kind,
            enabled: enabled.as_bool(),
            checked: checked.as_bool(),
            shortcut: (!shortcut.is_empty()).then_some(shortcut),
            children,
        });
    }
    Ok(items)
}

/// Prints the name of every item the engine offered, kept or not, which is
/// how the allowlist's names are found: the engine documents only examples.
#[cfg(debug_assertions)]
fn log_item_names(items: &[NativeMenuItem], depth: usize) {
    for item in items {
        eprintln!("[context menu] {}{} ({:?})", "  ".repeat(depth), item.name, item.kind);
        log_item_names(&item.children, depth + 1);
    }
}

unsafe fn read_dialog(args: &ICoreWebView2ScriptDialogOpeningEventArgs) -> windows::core::Result<PageDialog> {
    let mut kind = COREWEBVIEW2_SCRIPT_DIALOG_KIND::default();
    args.Kind(&mut kind)?;
    let kind = match kind {
        COREWEBVIEW2_SCRIPT_DIALOG_KIND_CONFIRM => DialogKind::Confirm,
        COREWEBVIEW2_SCRIPT_DIALOG_KIND_PROMPT => DialogKind::Prompt,
        COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD => DialogKind::BeforeUnload,
        _ => DialogKind::Alert,
    };

    let mut message = PWSTR::null();
    args.Message(&mut message)?;
    let mut default_text = PWSTR::null();
    args.DefaultText(&mut default_text)?;
    let mut url = PWSTR::null();
    args.Uri(&mut url)?;

    Ok(PageDialog {
        id: DialogId(NEXT_DIALOG.fetch_add(1, Ordering::Relaxed)),
        kind,
        message: take_pwstr(message),
        default_text: take_pwstr(default_text),
        url: take_pwstr(url),
    })
}

pub fn forget_page<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    let slot = SlotId::from_label(webview.label());
    let (sender, receiver) = mpsc::channel();
    webview.with_webview(move |platform| {
        if let Ok(core) = unsafe { platform.controller().CoreWebView2() } {
            let watched = WORKERS.with(|state| state.borrow().get(&true).is_some_and(|watch| watch.core == core));
            if watched {
                release_worker_watch(true);
            }
        }
        let windows: Vec<_> = PENDING_WINDOWS.with(|pending| {
            pending
                .borrow_mut()
                .extract_if(|_, (_, _, owner)| *owner == slot)
                .map(|(_, entry)| entry)
                .collect()
        });
        for (_, deferral, _) in windows {
            let _ = unsafe { deferral.Complete() };
        }
        let dialogs: Vec<_> = PENDING_DIALOGS.with(|pending| {
            pending
                .borrow_mut()
                .extract_if(|_, (_, _, owner)| *owner == slot)
                .map(|(_, entry)| entry)
                .collect()
        });
        for (_, deferral, _) in dialogs {
            let _ = unsafe { deferral.Complete() };
        }
        let menus: Vec<_> = PENDING_MENUS.with(|pending| {
            pending
                .borrow_mut()
                .extract_if(|_, menu| menu.slot == slot)
                .map(|(_, menu)| menu)
                .collect()
        });
        for menu in menus {
            let _ = unsafe { menu.deferral.Complete() };
        }
        // The engine stops reporting a download whose webview is gone, and
        // no longer lets it be acted on.
        DOWNLOADS.with(|downloads| downloads.borrow_mut().retain(|_, followed| followed.slot != slot));
        RESUMED.with(|resumed| resumed.borrow_mut().retain(|(_, _, owner)| *owner != slot));
        let _ = sender.send(());
    })?;
    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))
}

pub fn is_private<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<bool> {
    let (sender, receiver) = mpsc::channel();
    webview.with_webview(move |platform| {
        let outcome = unsafe {
            (|| -> windows::core::Result<bool> {
                let profile = platform
                    .controller()
                    .CoreWebView2()?
                    .cast::<ICoreWebView2_13>()?
                    .Profile()?;
                let mut enabled = BOOL::default();
                profile.IsInPrivateModeEnabled(&mut enabled)?;
                Ok(enabled.as_bool())
            })()
        }
        .map_err(|_| HakuError::Unsupported("runtime cannot verify private mode".into()));
        let _ = sender.send(outcome);
    })?;
    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

struct WorkerWatch {
    core: ICoreWebView2,
    tracker: WorkerTracker,
    receiver: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2DevToolsProtocolEventReceiver,
    token: i64,
}

fn release_worker_watch(private: bool) {
    WORKERS.with(|state| {
        if let Some(watch) = state.borrow_mut().remove(&private) {
            unsafe {
                let _ = watch.receiver.remove_DevToolsProtocolEventReceived(watch.token);
                let _ = watch.core.CallDevToolsProtocolMethod(
                    &HSTRING::from("ServiceWorker.disable"),
                    &HSTRING::from("{}"),
                    &ignore_result(),
                );
            }
        }
    });
}

pub fn stop_idle_workers<R: tauri::Runtime>(chrome: &tauri::Webview<R>) -> Result<()> {
    watch_workers(chrome, false)
}

pub fn watch_private_workers<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    webview: Option<&tauri::Webview<R>>,
) -> Result<()> {
    if let Some(webview) = webview {
        return watch_workers(webview, true);
    }
    app.run_on_main_thread(|| release_worker_watch(true))?;
    Ok(())
}

fn watch_workers<R: tauri::Runtime>(webview: &tauri::Webview<R>, private: bool) -> Result<()> {
    let (sender, receiver) = mpsc::channel();
    let app = webview.app_handle().clone();
    webview.with_webview(move |platform| {
        let outcome = unsafe { attach_worker_watch(&platform.controller(), app, private) }
            .map_err(|error| HakuError::WindowMissing(error.to_string()));
        let _ = sender.send(outcome);
    })?;
    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

unsafe fn attach_worker_watch<R: tauri::Runtime>(
    controller: &ICoreWebView2Controller,
    app: tauri::AppHandle<R>,
    private: bool,
) -> windows::core::Result<()> {
    let core = controller.CoreWebView2()?;
    if WORKERS.with(|state| state.borrow().get(&private).is_some_and(|watch| watch.core == core)) {
        return Ok(());
    }
    release_worker_watch(private);
    static NEXT_WATCH: AtomicU64 = AtomicU64::new(1);
    let generation = NEXT_WATCH.fetch_add(1, Ordering::Relaxed);
    let receiver = core.GetDevToolsProtocolEventReceiver(&HSTRING::from("ServiceWorker.workerVersionUpdated"))?;
    let on_update = DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut json = PWSTR::null();
        args.ParameterObjectAsJson(&mut json)?;
        let json = take_pwstr(json);
        let idle = WORKERS.with(|state| {
            state
                .borrow_mut()
                .get_mut(&private)
                .map(|watch| watch.tracker.update(generation, &json))
                .unwrap_or_default()
        });
        for (version, token) in idle {
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(IDLE_GRACE);
                let _ = app.run_on_main_thread(move || stop_if_still_idle(private, generation, &version, token));
            });
        }
        Ok(())
    }));
    let mut token = 0i64;
    receiver.add_DevToolsProtocolEventReceived(&on_update, &mut token)?;
    WORKERS.with(|state| {
        state.borrow_mut().insert(
            private,
            WorkerWatch {
                core: core.clone(),
                tracker: WorkerTracker::new(generation),
                receiver,
                token,
            },
        )
    });
    let enabled = core.CallDevToolsProtocolMethod(
        &HSTRING::from("ServiceWorker.enable"),
        &HSTRING::from("{}"),
        &ignore_result(),
    );
    if enabled.is_err() {
        release_worker_watch(private);
    }
    enabled
}

fn stop_if_still_idle(private: bool, generation: u64, version: &str, token: u64) {
    WORKERS.with(|state| {
        let mut state = state.borrow_mut();
        let Some(watch) = state.get_mut(&private) else {
            return;
        };
        if !watch.tracker.confirm(generation, version, token) {
            return;
        }
        unsafe {
            let _ = watch.core.CallDevToolsProtocolMethod(
                &HSTRING::from("ServiceWorker.stopWorker"),
                &HSTRING::from(workers::stop_params(version)),
                &ignore_result(),
            );
        }
    });
}

/// A DevTools call nobody waits on. A stop that fails leaves the worker
/// running, which is no worse than not having asked.
fn ignore_result() -> ICoreWebView2CallDevToolsProtocolMethodCompletedHandler {
    CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(())))
}

pub fn answer_dialog<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: DialogId, answer: DialogAnswer) -> Result<()> {
    app.run_on_main_thread(move || {
        let Some((args, deferral, _)) = PENDING_DIALOGS.with(|pending| pending.borrow_mut().remove(&id)) else {
            return;
        };
        unsafe {
            if let DialogAnswer::Accept { text } = answer {
                if let Some(text) = text {
                    let _ = args.SetResultText(&HSTRING::from(text));
                }
                let _ = args.Accept();
            }
            let _ = deferral.Complete();
        }
    })?;
    Ok(())
}

pub fn answer_context_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: ContextMenuRequestId,
    choice: Option<i32>,
) -> Result<()> {
    app.run_on_main_thread(move || {
        let Some(menu) = PENDING_MENUS.with(|pending| pending.borrow_mut().remove(&request)) else {
            return;
        };
        unsafe {
            // Only an item the page offered: the id is the engine's, and an
            // unknown one could run any of its commands.
            if let Some(choice) = choice.filter(|choice| offers(&menu.items, *choice)) {
                let _ = menu.args.SetSelectedCommandId(choice);
            }
            let _ = menu.deferral.Complete();
        }
    })?;
    Ok(())
}

pub fn adopt_window<R: tauri::Runtime>(webview: &tauri::Webview<R>, request: WindowRequestId) -> Result<bool> {
    let (sender, receiver) = mpsc::channel();
    webview
        .with_webview(move |platform| {
            let Some((args, deferral, _)) = PENDING_WINDOWS.with(|pending| pending.borrow_mut().remove(&request))
            else {
                let _ = sender.send(false);
                return;
            };
            let adopted = unsafe {
                let adopted = platform
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| args.SetNewWindow(&core))
                    .is_ok();
                let _ = deferral.Complete();
                adopted
            };
            let _ = sender.send(adopted);
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))
}

pub fn refuse_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>, request: WindowRequestId) -> Result<()> {
    app.run_on_main_thread(move || {
        if let Some((_, deferral, _)) = PENDING_WINDOWS.with(|pending| pending.borrow_mut().remove(&request)) {
            let _ = unsafe { deferral.Complete() };
        }
    })?;
    Ok(())
}

/// Collects the page's navigation log and forwards it.
///
/// `new_document` is set when a URL change triggered the read. It decides the
/// fallback where the page has no log, such as an error page or the PDF viewer:
/// a new document is treated as a new entry, anything else as a replacement.
unsafe fn drain(core: &ICoreWebView2, sink: PageSink, new_document: Option<bool>) -> windows::core::Result<()> {
    let mut source = PWSTR::null();
    core.Source(&mut source)?;
    let source = take_pwstr(source);
    // A parked or freshly created slot shows a blank page. It belongs to no
    // tab's history, and its title is not a page title.
    if source == BLANK_URL {
        return Ok(());
    }
    let mut title = PWSTR::null();
    core.DocumentTitle(&mut title)?;
    let title = take_pwstr(title);

    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |status, result| {
        let drained = status.ok().and_then(|()| inject::parse_navigation_log(&result));
        let signal = match drained {
            Some((commits, title)) => PageSignal::Changed {
                commits,
                title: Some(title),
            },
            None => {
                let commits = match new_document {
                    Some(new) => {
                        let kind = if new {
                            NavigationKind::Push
                        } else {
                            NavigationKind::Replace
                        };
                        vec![Commit { url: source, kind }]
                    }
                    None => Vec::new(),
                };
                PageSignal::Changed {
                    commits,
                    title: Some(title),
                }
            }
        };
        sink(signal);
        Ok(())
    }));
    core.ExecuteScript(&HSTRING::from(inject::navigation_drain_expression()), &handler)
}

/// Runs `start` against the chrome's profile on the UI thread and waits for
/// the result it sends once the engine's callback arrives.
///
/// Every webview shares the profile, so an extension installed through it
/// runs in every page.
fn with_profile<R, T, F>(chrome: &tauri::Webview<R>, step: &'static str, start: F) -> Result<T>
where
    R: tauri::Runtime,
    T: Send + 'static,
    F: FnOnce(ICoreWebView2Profile7, mpsc::Sender<Result<T>>) -> windows::core::Result<()> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    chrome
        .with_webview(move |platform| {
            let profile = unsafe {
                platform
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.cast::<ICoreWebView2_13>())
                    .and_then(|core| core.Profile())
                    .and_then(|profile| profile.cast::<ICoreWebView2Profile7>())
            };
            let started = profile.and_then(|profile| start(profile, sender.clone()));
            if let Err(error) = started {
                let _ = sender.send(Err(engine_error(step, &error)));
            }
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|_| HakuError::Unsupported(format!("{step}: the engine did not answer")))?
}

fn engine_error(step: &str, error: &windows::core::Error) -> HakuError {
    HakuError::Unsupported(format!("{step}: {error}"))
}

pub fn install_extension<R: tauri::Runtime>(chrome: &tauri::Webview<R>, folder: &Path) -> Result<(String, String)> {
    let folder = HSTRING::from(folder);
    with_profile(chrome, "installing", move |profile, sender| unsafe {
        let handler = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |status, extension| {
            let outcome = status.and_then(|()| {
                let extension = extension.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                let (mut id, mut name) = (PWSTR::null(), PWSTR::null());
                extension.Id(&mut id)?;
                extension.Name(&mut name)?;
                let (id, name) = (take_pwstr(id), take_pwstr(name));
                // Kept so it can be switched without listing the profile's
                // extensions again. The handler runs on the UI thread, where
                // the map lives.
                EXTENSIONS.with(|extensions| extensions.borrow_mut().insert(id.clone(), extension));
                Ok((id, name))
            });
            let _ = sender.send(outcome.map_err(|error| engine_error("installing", &error)));
            Ok(())
        }));
        profile.AddBrowserExtension(&folder, &handler)
    })
}

pub fn set_extension_enabled<R: tauri::Runtime>(chrome: &tauri::Webview<R>, id: String, enabled: bool) -> Result<()> {
    let step = if enabled { "enabling" } else { "disabling" };
    with_profile(chrome, step, move |_, sender| unsafe {
        let Some(extension) = EXTENSIONS.with(|extensions| extensions.borrow().get(&id).cloned()) else {
            // Not installed by this process: nothing to switch.
            let _ = sender.send(Ok(()));
            return Ok(());
        };
        let done = move |status: windows::core::Result<()>| {
            let _ = sender.send(status.map_err(|error| engine_error(step, &error)));
            Ok(())
        };
        extension.Enable(enabled, &BrowserExtensionEnableCompletedHandler::create(Box::new(done)))
    })
}

pub fn uninstall_extension<R: tauri::Runtime>(chrome: &tauri::Webview<R>, id: String) -> Result<()> {
    with_profile(chrome, "uninstalling", move |profile, sender| unsafe {
        let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |status, list| {
            let found = status.and_then(|()| {
                let Some(list) = list else { return Ok(None) };
                let mut count = 0u32;
                list.Count(&mut count)?;
                for index in 0..count {
                    let extension = list.GetValueAtIndex(index)?;
                    let mut listed = PWSTR::null();
                    extension.Id(&mut listed)?;
                    if take_pwstr(listed) == id {
                        return Ok(Some(extension));
                    }
                }
                Ok(None)
            });
            let removing = match found {
                // Already gone: nothing to remove.
                Ok(None) => {
                    let _ = sender.send(Ok(()));
                    return Ok(());
                }
                Ok(Some(extension)) => {
                    // Forgotten only if still the one cached now: an install of
                    // the same id may have cached another by the time it is done.
                    let cached = EXTENSIONS.with(|extensions| extensions.borrow().get(&id).cloned());
                    let done = {
                        let sender = sender.clone();
                        let id = id.clone();
                        BrowserExtensionRemoveCompletedHandler::create(Box::new(move |status| {
                            if status.is_ok() {
                                EXTENSIONS.with(|extensions| {
                                    let mut extensions = extensions.borrow_mut();
                                    if extensions.get(&id) == cached.as_ref() {
                                        extensions.remove(&id);
                                    }
                                });
                            }
                            let _ = sender.send(status.map_err(|error| engine_error("uninstalling", &error)));
                            Ok(())
                        }))
                    };
                    extension.Remove(&done)
                }
                Err(error) => Err(error),
            };
            if let Err(error) = removing {
                let _ = sender.send(Err(engine_error("uninstalling", &error)));
            }
            Ok(())
        }));
        profile.GetBrowserExtensions(&handler)
    })
}

pub fn pick_extension<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    source: crate::model::ExtensionSource,
) -> Result<Option<std::path::PathBuf>> {
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
    };

    let picker_error = |error: windows::core::Error| HakuError::Unsupported(format!("picking an extension: {error}"));
    // A window handle is not `Send`; its value is all the picker needs.
    let owner = window
        .hwnd()
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
        .0 as isize;
    // A thread of its own, so the picker gets the single-threaded COM
    // apartment it needs whatever the caller's thread has.
    let picked = std::thread::spawn(move || unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let picked = (|| {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            let options = dialog.GetOptions()? | FOS_FORCEFILESYSTEM;
            match source {
                crate::model::ExtensionSource::Folder => dialog.SetOptions(options | FOS_PICKFOLDERS)?,
                crate::model::ExtensionSource::File => {
                    // Named by its patterns, which read the same in every language.
                    dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                        pszName: windows::core::w!("*.crx, *.zip"),
                        pszSpec: windows::core::w!("*.crx;*.zip"),
                    }])?;
                    dialog.SetOptions(options | FOS_FILEMUSTEXIST)?;
                }
            }
            match dialog.Show(Some(HWND(owner as *mut _))) {
                Err(error) if error.code() == ERROR_CANCELLED.to_hresult() => return Ok(None),
                shown => shown?,
            }
            let path = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            Ok(Some(std::path::PathBuf::from(take_pwstr(path))))
        })();
        CoUninitialize();
        picked
    })
    .join()
    .map_err(|_| HakuError::Unsupported("picking an extension: the picker failed".into()))?;
    picked.map_err(picker_error)
}

pub fn reveal_folder(folder: &Path) -> Result<()> {
    std::process::Command::new("explorer")
        .arg(folder)
        .spawn()
        .map(drop)
        .map_err(|error| HakuError::Unsupported(format!("opening {}: {error}", folder.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(reason: i32) -> Option<DownloadFailure> {
        interruption(COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON(reason)).1
    }

    #[test]
    fn interrupt_reasons_name_the_failure_by_the_engine_numbers() {
        assert_eq!(failure(1), Some(DownloadFailure::File));
        assert_eq!(failure(11), Some(DownloadFailure::File));
        assert_eq!(failure(12), Some(DownloadFailure::Network));
        assert_eq!(failure(16), Some(DownloadFailure::Network));
        assert_eq!(failure(17), Some(DownloadFailure::Server));
        assert_eq!(failure(22), Some(DownloadFailure::Server));
        assert_eq!(failure(25), Some(DownloadFailure::Server));
        assert_eq!(failure(29), Some(DownloadFailure::Other));
        assert_eq!(
            interruption(COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON(28)),
            (DownloadState::Paused, None)
        );
        assert_eq!(
            interruption(COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON(26)),
            (DownloadState::Cancelled, None)
        );
    }
}
