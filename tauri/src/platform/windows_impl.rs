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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tauri::Manager;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2CallDevToolsProtocolMethodCompletedHandler, ICoreWebView2Controller,
    ICoreWebView2Deferral, ICoreWebView2Environment13, ICoreWebView2FrameInfo, ICoreWebView2FrameInfo2,
    ICoreWebView2NavigationStartingEventArgs3, ICoreWebView2ProcessExtendedInfoCollection,
    ICoreWebView2ScriptDialogOpeningEventArgs, ICoreWebView2_19, ICoreWebView2_2, ICoreWebView2_20, ICoreWebView2_3,
    ICoreWebView2_8, COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_JPEG, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL,
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    COREWEBVIEW2_NAVIGATION_KIND, COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD, COREWEBVIEW2_PROCESS_KIND,
    COREWEBVIEW2_PROCESS_KIND_BROWSER, COREWEBVIEW2_PROCESS_KIND_GPU, COREWEBVIEW2_PROCESS_KIND_RENDERER,
    COREWEBVIEW2_PROCESS_KIND_UTILITY, COREWEBVIEW2_SCRIPT_DIALOG_KIND, COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD,
    COREWEBVIEW2_SCRIPT_DIALOG_KIND_CONFIRM, COREWEBVIEW2_SCRIPT_DIALOG_KIND_PROMPT,
};
use webview2_com::{
    take_pwstr, CallDevToolsProtocolMethodCompletedHandler, CapturePreviewCompletedHandler,
    DOMContentLoadedEventHandler, DevToolsProtocolEventReceivedEventHandler, DocumentTitleChangedEventHandler,
    ExecuteScriptCompletedHandler, GetProcessExtendedInfosCompletedHandler, IsDocumentPlayingAudioChangedEventHandler,
    NavigationStartingEventHandler, ScriptDialogOpeningEventHandler, SourceChangedEventHandler,
    TrySuspendCompletedHandler,
};
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HRGN, RGN_DIFF, RGN_OR,
};
use windows::Win32::System::Com::{IStream, STREAM_SEEK_END, STREAM_SEEK_SET};
use windows::Win32::System::ProcessStatus::{
    GetPerformanceInfo, GetProcessMemoryInfo, PERFORMANCE_INFORMATION, PROCESS_MEMORY_COUNTERS,
    PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
};

use super::memory::{EngineSnapshot, ProcessKind, ProcessSample};
use super::workers::{self, WorkerTracker, IDLE_GRACE};
use super::{PageSignal, PageSink, PhysicalRect, RoundedRect};
use crate::browser::BLANK_URL;
use crate::error::{HakuError, Result};
use crate::model::{Commit, DialogAnswer, DialogId, DialogKind, MemoryStatus, NavigationKind, PageDialog, SlotId};
use crate::webview::inject;

/// How long to wait for the main thread to run a native operation.
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Source of dialog ids, unique for the life of the process.
static NEXT_DIALOG: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Dialogs pages are paused on, awaiting an answer.
    ///
    /// Thread-local because the WebView2 objects involved belong to the UI
    /// thread: they are stored by the handler that runs there and taken back by
    /// the answer, which is dispatched there too.
    static PENDING_DIALOGS: RefCell<HashMap<DialogId, (ICoreWebView2ScriptDialogOpeningEventArgs, ICoreWebView2Deferral)>> =
        RefCell::new(HashMap::new());

    /// The chrome webview's DevTools session and what it has seen of the
    /// profile's service workers. UI-thread only, like the webview itself.
    static WORKERS: RefCell<Option<(ICoreWebView2, WorkerTracker)>> = const { RefCell::new(None) };
}

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
    viewport: Option<PhysicalRect>,
    radius: i32,
    overlays: &[RoundedRect],
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
        let hole = rounded_region(viewport, radius);
        CombineRgn(Some(region), Some(region), Some(hole), RGN_DIFF);
        let _ = DeleteObject(hole.into());

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

pub fn observe_page<R: tauri::Runtime>(webview: &tauri::Webview<R>, sink: PageSink) -> Result<()> {
    let (sender, receiver) = mpsc::channel();

    webview
        .with_webview(move |platform| {
            let outcome = unsafe { attach_observers(&platform.controller(), &sink) }
                .map_err(|error| HakuError::WindowMissing(error.to_string()));
            let _ = sender.send(outcome);
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

unsafe fn attach_observers(controller: &ICoreWebView2Controller, sink: &PageSink) -> windows::core::Result<()> {
    let core = controller.CoreWebView2()?;
    // Handlers stay registered for the webview's whole life, so the tokens that
    // would unregister them are never needed.
    let mut token = 0i64;

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
            PENDING_DIALOGS.with(|pending| pending.borrow_mut().insert(dialog.id, (args, deferral)));
            sink(PageSignal::DialogRequested(dialog));
            Ok(())
        }))
    };
    core.add_ScriptDialogOpening(&on_dialog, &mut token)?;

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
    Ok(())
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

pub fn stop_idle_workers<R: tauri::Runtime>(chrome: &tauri::Webview<R>) -> Result<()> {
    let (sender, receiver) = mpsc::channel();
    let app = chrome.app_handle().clone();

    chrome
        .with_webview(move |platform| {
            let outcome = unsafe { attach_worker_watch(&platform.controller(), app) }
                .map_err(|error| HakuError::WindowMissing(error.to_string()));
            let _ = sender.send(outcome);
        })
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?;

    receiver
        .recv_timeout(DISPATCH_TIMEOUT)
        .map_err(|error| HakuError::WindowMissing(error.to_string()))?
}

unsafe fn attach_worker_watch<R: tauri::Runtime>(
    controller: &ICoreWebView2Controller,
    app: tauri::AppHandle<R>,
) -> windows::core::Result<()> {
    let core = controller.CoreWebView2()?;
    WORKERS.with(|state| *state.borrow_mut() = Some((core.clone(), WorkerTracker::default())));

    let on_update = DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut json = PWSTR::null();
        args.ParameterObjectAsJson(&mut json)?;
        let json = take_pwstr(json);

        let idle = WORKERS.with(|state| {
            state
                .borrow_mut()
                .as_mut()
                .map(|(_, tracker)| tracker.update(&json))
                .unwrap_or_default()
        });
        for (version, token) in idle {
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(IDLE_GRACE);
                let _ = app.run_on_main_thread(move || stop_if_still_idle(&version, token));
            });
        }
        Ok(())
    }));
    let mut token = 0i64;
    core.GetDevToolsProtocolEventReceiver(&HSTRING::from("ServiceWorker.workerVersionUpdated"))?
        .add_DevToolsProtocolEventReceived(&on_update, &mut token)?;

    // Updates only flow once the domain is enabled, and it stays enabled for
    // the life of the webview's DevTools session.
    core.CallDevToolsProtocolMethod(
        &HSTRING::from("ServiceWorker.enable"),
        &HSTRING::from("{}"),
        &ignore_result(),
    )
}

/// Stops a worker the grace period has expired on, unless a page took it back.
fn stop_if_still_idle(version: &str, token: u64) {
    WORKERS.with(|state| {
        let mut state = state.borrow_mut();
        let Some((core, tracker)) = state.as_mut() else { return };
        if !tracker.confirm(version, token) {
            return;
        }
        let params = HSTRING::from(workers::stop_params(version));
        unsafe {
            let _ =
                core.CallDevToolsProtocolMethod(&HSTRING::from("ServiceWorker.stopWorker"), &params, &ignore_result());
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
        let Some((args, deferral)) = PENDING_DIALOGS.with(|pending| pending.borrow_mut().remove(&id)) else {
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
