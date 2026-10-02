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
use std::time::Duration;

use tauri::Manager;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2CallDevToolsProtocolMethodCompletedHandler, ICoreWebView2Controller, ICoreWebView2Deferral, ICoreWebView2NavigationStartingEventArgs3,
    ICoreWebView2ScriptDialogOpeningEventArgs, ICoreWebView2_19, ICoreWebView2_3, ICoreWebView2_8,
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL, COREWEBVIEW2_NAVIGATION_KIND,
    COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD, COREWEBVIEW2_SCRIPT_DIALOG_KIND,
    COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD, COREWEBVIEW2_SCRIPT_DIALOG_KIND_CONFIRM,
    COREWEBVIEW2_SCRIPT_DIALOG_KIND_PROMPT,
};
use webview2_com::{
    take_pwstr, CallDevToolsProtocolMethodCompletedHandler, DevToolsProtocolEventReceivedEventHandler,
    DocumentTitleChangedEventHandler, ExecuteScriptCompletedHandler, IsDocumentPlayingAudioChangedEventHandler,
    NavigationStartingEventHandler, ScriptDialogOpeningEventHandler, SourceChangedEventHandler,
    TrySuspendCompletedHandler,
};
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HRGN, RGN_DIFF, RGN_OR,
};
use windows::Win32::System::Memory::{
    CreateMemoryResourceNotification, LowMemoryResourceNotification, QueryMemoryResourceNotification,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
};

use super::workers::{self, WorkerTracker, IDLE_GRACE};
use super::{PageSignal, PageSink, PhysicalRect, RoundedRect};
use crate::browser::BLANK_URL;
use crate::error::{HakuError, Result};
use crate::model::{Commit, DialogAnswer, DialogId, DialogKind, NavigationKind, PageDialog};
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

pub fn total_memory() -> Option<u64> {
    let mut status = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    Some(status.ullTotalPhys)
}

pub fn memory_is_low() -> bool {
    // Created once and kept for the life of the process. Stored as an integer
    // because a raw handle is not shareable between threads, though the
    // notification object behind it is.
    static NOTIFICATION: std::sync::OnceLock<Option<isize>> = std::sync::OnceLock::new();
    let handle = NOTIFICATION.get_or_init(|| {
        unsafe { CreateMemoryResourceNotification(LowMemoryResourceNotification) }
            .ok()
            .map(|handle| handle.0 as isize)
    });
    let Some(handle) = *handle else { return false };

    let mut low = BOOL::default();
    unsafe { QueryMemoryResourceNotification(HANDLE(handle as *mut _), &mut low) }.is_ok() && low.as_bool()
}

pub fn raise_chrome<R: tauri::Runtime>(webview: &tauri::Webview<R>) -> Result<()> {
    with_hwnd(webview, |hwnd| unsafe {
        SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
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
            let _ = DeleteObject(HRGN::from(region).into());
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
            let Ok(args) = args.cast::<ICoreWebView2NavigationStartingEventArgs3>() else { return Ok(()) };

            let mut kind = COREWEBVIEW2_NAVIGATION_KIND::default();
            args.NavigationKind(&mut kind)?;
            // Haku never traverses a webview's native history itself, so any
            // back or forward here came from the page, a key or a mouse button.
            if kind == COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD {
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                args.SetCancel(true)?;
                sink(PageSignal::TraverseRequested { url: take_pwstr(uri) });
            }
            Ok(())
        }))
    };
    core.add_NavigationStarting(&on_starting, &mut token)?;

    let on_source = {
        let sink = sink.clone();
        SourceChangedEventHandler::create(Box::new(move |sender, args| {
            let (Some(core), Some(args)) = (sender, args) else { return Ok(()) };
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
                sink(PageSignal::AudioChanged { playing: playing.as_bool() });
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
            state.borrow_mut().as_mut().map(|(_, tracker)| tracker.update(&json)).unwrap_or_default()
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
    core.CallDevToolsProtocolMethod(&HSTRING::from("ServiceWorker.enable"), &HSTRING::from("{}"), &ignore_result())
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
            let _ = core.CallDevToolsProtocolMethod(&HSTRING::from("ServiceWorker.stopWorker"), &params, &ignore_result());
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
            Some((commits, title)) => PageSignal::Changed { commits, title: Some(title) },
            None => {
                let commits = match new_document {
                    Some(new) => {
                        let kind = if new { NavigationKind::Push } else { NavigationKind::Replace };
                        vec![Commit { url: source, kind }]
                    }
                    None => Vec::new(),
                };
                PageSignal::Changed { commits, title: Some(title) }
            }
        };
        sink(signal);
        Ok(())
    }));
    core.ExecuteScript(&HSTRING::from(inject::navigation_drain_expression()), &handler)
}
