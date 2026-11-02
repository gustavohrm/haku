//! The command surface the interface drives Haku through.
//!
//! ## Trust
//!
//! Application commands are reachable from any webview in the process, and
//! content webviews load arbitrary remote pages. Every command here therefore
//! rejects callers that are not a window's chrome, and acts for the window
//! whose chrome called it.
//!
//! ## Threading
//!
//! Every command that drives a webview is `async`. Tauri runs synchronous
//! commands on the main thread, and creating, moving or raising a webview
//! dispatches work to that same thread and waits for it, which deadlocks the
//! event loop. Commands that only read state stay synchronous.
//!
//! Content webviews have no channel of their own: Tauri withholds its IPC
//! bridge from remote origins unless a capability opts them in, and Haku does
//! not. What a page is showing is observed from Rust instead, through
//! [`page_observer`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Duration;

use tauri::{Manager, State};
use tauri_specta::Event;

use crate::browser::Ending;
use crate::browser::{resolve_target, Browser, BrowserState, Direction, Effect, Opening, PageReport};
use crate::chrome::{self, Layout};
use crate::error::{HakuError, Result};
use crate::model::{
    check_query, is_internal, jpeg_data_url, keymap, AddedExtension, ContextMenuRequestId, DialogAnswer, DialogId,
    Download, DownloadId, DownloadState, ExtensionSource, Extensions, FindDirection, PageMenu, PermissionDecision,
    PermissionRequestId, Placement, Preset, Pressure, Profile, ScreenRect, Shortcut, SlotId, TabId, WindowId,
    WindowKind, WindowRequestId,
};
use crate::platform::{self, DownloadAction, KeySink, PageSignal, ProcessFailure};
use crate::state::{now_ms, AppState, MemoryReport};
use crate::storage::history_db::HistoryEntry;
use crate::storage::settings::{key_bindings, KeyBinding};
use crate::storage::{Settings, WindowBounds};
use crate::webview::inject::ExtensionView;
use crate::webview::{self, Departure, Hooks, PageObserver, PageReading};

use super::events::{
    AddressFocusRequested, CloseConfirmationRequested, ContextMenuRequested, FailureReported, FindEnded, FindRequested,
    FindResult, MemoryChanged, SettingsChanged, StateChanged,
};
use super::shortcuts::{self, PageAction, RecordedChord, Request as ShortcutRequest};
use super::{downloads, extensions, recovery};

/// Rejects a command that only the interface may issue.
///
/// @returns The window whose chrome called.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the caller is a content webview.
fn ensure_chrome(webview: &tauri::Webview) -> Result<WindowId> {
    WindowId::from_label(webview.label())
        .ok_or_else(|| HakuError::Unsupported(format!("{} may not call browser commands", webview.label())))
}

/// What every webview Haku creates is given to report through.
pub fn hooks() -> Hooks<tauri::Wry> {
    Hooks {
        page: page_observer(),
        keys: Arc::new(|app: &tauri::AppHandle, window: WindowId| shortcut_sink(app.clone(), window)),
        failures: Arc::new(recovery::chrome_failures),
    }
}

/// Observes what content webviews load, without the pages taking part.
///
/// Tauri withholds its IPC bridge from remote origins unless a capability opts
/// them in, and Haku deliberately does not. What a page shows is read from Rust
/// instead; see [`crate::webview::inject`] for how.
///
/// Runs on the UI thread. Anything that drives a webview is moved off it, for
/// the same reason commands that do are `async`: webview operations dispatch to
/// the UI thread and wait, and waiting on it from it never returns.
fn page_observer() -> PageObserver<tauri::Wry> {
    Arc::new(
        |app: &tauri::AppHandle, slot: SlotId, signal: PageSignal| match signal {
            PageSignal::Changed { commits, title } => record_page(app, slot, &commits, title),
            PageSignal::TraverseRequested { url } => {
                let app = app.clone();
                std::thread::spawn(move || traverse(&app, slot, &url));
            }
            PageSignal::DialogRequested(dialog) => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let _ = mutate(&app, &state, |browser| Ok(browser.open_dialog(slot, dialog)));
                });
            }
            PageSignal::PermissionRequested(request) => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let _ = mutate(&app, &state, |browser| Ok(browser.request_permission(slot, request)));
                });
            }
            // Recorded before the page commits, on this thread, so the
            // navigation it belongs to finds it.
            PageSignal::NavigationStarted { form } => {
                let ended = app.state::<AppState>().browser.write().ok().and_then(|mut browser| {
                    browser.report_navigation(slot, form);
                    let tab = browser.tab_in(slot)?;
                    Some((tab, browser.window_of(tab)?))
                });
                // The engine ends a find on navigation; the bar closes with it.
                if let Some((tab, window)) = ended {
                    let _ = FindEnded(tab).emit_to(app, window.label());
                }
            }
            // Handled by the webview module, which holds what is restored.
            PageSignal::Loaded { .. } => {}
            // The cover comes off once the page has loaded and, for a tab
            // getting its scroll back, once it has scrolled.
            PageSignal::Completed { url } => {
                let state = app.state::<AppState>();
                let loaded = state
                    .browser
                    .write()
                    .ok()
                    .and_then(|mut browser| browser.report_loaded(slot, &url).then(|| window_states(&browser)));
                if let Some(states) = loaded {
                    emit_states(app, states);
                }
            }
            PageSignal::Shortcut(shortcut) => {
                let request = app
                    .state::<AppState>()
                    .browser
                    .read()
                    .ok()
                    .and_then(|browser| ShortcutRequest::page(&browser, slot));
                if let Some(request) = request {
                    queue_shortcut(app, request, shortcut);
                }
            }
            PageSignal::Focused => {
                let state = app.state::<AppState>();
                let focused = state.browser.write().ok().and_then(|mut browser| {
                    let target = browser.focus_slot(slot)?;
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "[page focus] slot={} window={} viewport={}",
                        slot.0, target.window.0, target.id
                    );
                    Some((target.window, browser.state_in(target.window)?))
                });
                if let Some(focused) = focused {
                    emit_states(app, vec![focused]);
                }
            }
            PageSignal::WindowRequested {
                request,
                url,
                popup,
                placement,
                background,
                window,
                gesture,
            } => queue_window(
                app,
                WindowRequest {
                    parent: None,
                    profile: Profile::Default,
                    session: 0,
                    slot: Some(slot),
                    request,
                    url,
                    popup,
                    placement,
                    background,
                    window,
                    gesture,
                },
            ),
            PageSignal::ProcessFailed(ProcessFailure::Page(cause)) => {
                // The tab whose page it was, read now: by the time the change
                // runs, the slot may hold another tab's page.
                let state = app.state::<AppState>();
                let Some(tab) = state.browser.read().ok().and_then(|browser| browser.tab_in(slot)) else {
                    return;
                };
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let _ = mutate(&app, &state, |browser| Ok(browser.page_crashed(slot, tab, cause)));
                });
            }
            PageSignal::ProcessFailed(ProcessFailure::Engine) => recovery::engine_failed(app),
            PageSignal::CloseRequested => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let Ok(home) = home_url(&state) else { return };
                    let _ = mutate(&app, &state, |browser| match browser.tab_in(slot) {
                        Some(id) => browser.close_tab(id, &home),
                        None => Ok(Vec::new()),
                    });
                });
            }
            PageSignal::ContextMenuRequested(menu) => {
                let app = app.clone();
                std::thread::spawn(move || offer_context_menu(&app, slot, menu));
            }
            PageSignal::FindChanged { matches, active } => {
                let state = app.state::<AppState>();
                let target = state.browser.read().ok().and_then(|browser| {
                    let tab = browser.tab_in(slot)?;
                    Some((tab, browser.window_of(tab)?))
                });
                if let Some((tab, window)) = target {
                    let _ = FindResult { tab, matches, active }.emit_to(app, window.label());
                }
            }
            PageSignal::FullscreenChanged { fullscreen } => {
                // Numbered here, on the UI thread, in the order the engine
                // reported them: the threads below may run in any order, and a
                // quick enter and exit must not end with the window fullscreen.
                let report = latest_fullscreen_reports().next(slot);
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let _ = mutate(&app, &state, |browser| {
                        if !latest_fullscreen_reports().is_latest(slot, report) {
                            return Ok(Vec::new());
                        }
                        Ok(browser.report_fullscreen(slot, fullscreen))
                    });
                });
            }
            PageSignal::ZoomChanged {
                percent,
                navigation,
                url,
            } => {
                // Numbered here, as fullscreen reports are: only the newest
                // says what the webview shows.
                let report = latest_zoom_reports().next(slot);
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let mut stored = None;
                    let _ = mutate(&app, &state, |browser| {
                        if !latest_zoom_reports().is_latest(slot, report) {
                            return Ok(Vec::new());
                        }
                        let (effects, changed) = browser.report_zoom(slot, percent, navigation, &url);
                        stored = changed
                            .then(|| browser.tab_in(slot).and_then(|tab| browser.window_of(tab)))
                            .flatten();
                        Ok(effects)
                    });
                    if let Some(window) = stored {
                        let _ = store_site_zoom(&app, &state, window);
                    }
                });
            }
            PageSignal::AudioChanged { playing } => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let _ = mutate(&app, &state, |browser| {
                        Ok(browser.report_audio(slot, playing, now_ms()))
                    });
                });
            }
        },
    )
}

/// The newest fullscreen report numbered for each slot.
#[derive(Default)]
struct FullscreenReports(HashMap<SlotId, u64>);

impl FullscreenReports {
    fn next(&mut self, slot: SlotId) -> u64 {
        let number = self.0.entry(slot).or_default();
        *number += 1;
        *number
    }

    fn is_latest(&self, slot: SlotId, report: u64) -> bool {
        self.0.get(&slot) == Some(&report)
    }
}

fn latest_fullscreen_reports() -> std::sync::MutexGuard<'static, FullscreenReports> {
    static REPORTS: OnceLock<Mutex<FullscreenReports>> = OnceLock::new();
    REPORTS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The newest zoom report numbered for each slot.
fn latest_zoom_reports() -> std::sync::MutexGuard<'static, FullscreenReports> {
    static REPORTS: OnceLock<Mutex<FullscreenReports>> = OnceLock::new();
    REPORTS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Stores the zoom levels in effect, as they stand when stored rather than
/// when they changed, so stores that overlap end with the newest levels.
/// [Zoom](../../../docs/specs/zoom.md).
fn store_site_zoom(app: &tauri::AppHandle, state: &AppState, window: WindowId) -> Result<()> {
    store_settings(app, state, window, |current| {
        let mut settings = current.clone();
        if let Ok(browser) = state.browser.read() {
            settings.site_zoom = crate::model::SiteZooms(browser.site_zoom().to_vec());
        }
        settings
    })
    .map(|_| ())
}

/// How long a page's context menu may go unanswered, say because its window
/// closed, before the page is released with no choice.
const MENU_EXPIRY: Duration = Duration::from_secs(60);

/// The tab each open page menu was asked for, and the slot that held it, so
/// an answer that arrives after the slot moved on to another tab's page
/// runs nothing there.
fn menu_owners() -> std::sync::MutexGuard<'static, HashMap<ContextMenuRequestId, (SlotId, TabId)>> {
    static OWNERS: OnceLock<Mutex<HashMap<ContextMenuRequestId, (SlotId, TabId)>>> = OnceLock::new();
    OWNERS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Sends a page's context menu to its tab's window, which draws it, and gives
/// that window's interface keyboard focus so the menu can be used from the
/// keyboard and closes when focus leaves it. Off the UI thread: it drives
/// webviews.
fn offer_context_menu(app: &tauri::AppHandle, slot: SlotId, menu: PageMenu) {
    let request = menu.request;
    // One offer at a time, newest last: each runs on its own thread, and a
    // request the engine has already replaced must not be drawn after it.
    static NEWEST: Mutex<u64> = Mutex::new(0);
    let mut newest = NEWEST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if request.0 <= *newest {
        drop(newest);
        let _ = platform::answer_context_menu(app, request, None);
        return;
    }
    *newest = request.0;
    let state = app.state::<AppState>();
    let owner = state.browser.read().ok().and_then(|browser| {
        let tab = browser.tab_in(slot)?;
        let target = browser.slot_viewports().remove(&slot)?;
        Some((tab, target))
    });
    let Some((tab, target)) = owner else {
        let _ = platform::answer_context_menu(app, request, None);
        return;
    };
    let sent = (|| -> Result<()> {
        let window = target.window;
        let chrome = webview::chrome_of(app, window)?;
        let scale = chrome.window().scale_factor()?;
        let viewport = state.viewports.get(window, &target.id);
        menu_owners().insert(request, (slot, tab));
        chrome.set_focus()?;
        ContextMenuRequested {
            request,
            tab,
            x: viewport.x + f64::from(menu.x) / scale,
            y: viewport.y + f64::from(menu.y) / scale,
            target: menu.target,
            items: menu.items,
        }
        .emit_to(app, window.label())
        .map_err(HakuError::from)
    })();
    if sent.is_err() {
        release_menu(app, request);
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(MENU_EXPIRY);
        release_menu(&app, request);
    });
}

/// Releases a page waiting on its context menu, with no choice. Does nothing
/// for a menu already answered.
fn release_menu(app: &tauri::AppHandle, request: ContextMenuRequestId) {
    menu_owners().remove(&request);
    let _ = platform::answer_context_menu(app, request, None);
}

/// A page's request for a new window, as [`PageSignal::WindowRequested`]
/// reported it from `slot`, or from an extension's toolbar popup without one.
struct WindowRequest {
    parent: Option<TabId>,
    profile: Profile,
    session: u64,
    slot: Option<SlotId>,
    request: WindowRequestId,
    url: String,
    popup: bool,
    placement: Placement,
    background: bool,
    window: bool,
    gesture: bool,
}

/// Answers pages' requests for new windows one at a time, in the order they
/// were made, so links opened for later line up in the order they were
/// clicked. Off the UI thread, like [`queue_shortcut`]: opening one drives
/// webviews.
fn queue_window(app: &tauri::AppHandle, mut window: WindowRequest) {
    if let Some(slot) = window.slot {
        let state = app.state::<AppState>();
        let source = state.browser.read().ok().and_then(|browser| {
            let parent = browser.tab_in(slot)?;
            Some((parent, browser.profile_of(parent), browser.private_session()))
        });
        let Some((parent, profile, session)) = source else {
            let _ = platform::refuse_window(app, window.request);
            return;
        };
        window.parent = Some(parent);
        window.profile = profile;
        window.session = session;
    }

    static QUEUE: OnceLock<mpsc::Sender<WindowRequest>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<WindowRequest>();
        let app = app.clone();
        std::thread::spawn(move || {
            for window in receiver {
                let request = window.request;
                let from_popup = window.slot.is_none();
                let opened = window.gesture;
                if opened {
                    open_requested(&app, window);
                }
                let _ = platform::refuse_window(&app, request);
                // What a toolbar popup opens takes the user away from it,
                // which closes it in other browsers, where opening anything
                // moves focus elsewhere. Only now, once the request has been
                // answered either way.
                if from_popup {
                    popup_request_answered(&app, opened);
                }
            }
        });
        sender
    });
    let _ = queue.send(window);
}

/// Opens what a page asked to open in a new window.
///
/// A popup opens in a window of its own, handed a new webview so the page can
/// still reach it. So is anything that is not an ordinary web address, such
/// as a blank page the page writes into or a `blob:` URL, which only the
/// engine can load on the page's behalf, though in a tab. A link to a web
/// address opens as an ordinary tab, or in a new browser window with Shift
/// held. A page never opens one of Haku's own pages. The request is answered
/// by the caller, if handing it a webview did not answer it.
///
/// As in other browsers, a page opens windows only in answer to the user:
/// a request the page made on its own never reaches here.
fn open_requested(app: &tauri::AppHandle, requested: WindowRequest) {
    let WindowRequest {
        parent,
        profile,
        session,
        request,
        url,
        popup,
        placement,
        background,
        window,
        ..
    } = requested;
    if is_internal(&url) {
        return;
    }
    let web = url.starts_with("https://") || url.starts_with("http://");
    let opening = if popup {
        Opening::Popup { request, placement }
    } else if !web {
        Opening::Connected(request)
    } else if window {
        Opening::Window
    } else if background {
        Opening::Background
    } else {
        Opening::Foreground
    };
    let state = app.state::<AppState>();
    let pressure = read_pressure(&state);
    // Read before opening: what opens may close again if it fails.
    let from = state.browser.read().ok().map(|browser| {
        parent
            .and_then(|parent| browser.window_of(parent))
            .unwrap_or_else(|| browser.main_window())
    });
    let opened = mutate(app, &state, |browser| {
        browser.set_pressure(pressure);
        if profile == Profile::Private && !browser.accepts_private_request(session) {
            return Ok(Vec::new());
        }
        Ok(browser.open_requested_profile(parent, profile, url, opening).1)
    });
    if let (Err(error), Some(from)) = (opened, from) {
        report_failure(app, from, error);
    }
}

/// Tells a window's interface about a failure no command call is waiting on,
/// as one it called would have been told.
fn report_failure(app: &tauri::AppHandle, window: WindowId, error: HakuError) {
    let _ = FailureReported(error).emit_to(app, window.label());
}

/// The toolbar popup's requests to open a window still being answered, and
/// whether it is to close once none is.
///
/// The popup waits on each request, and destroying it first crashes Haku
/// when the request is answered, so closing it meanwhile, for losing focus
/// to the window it asked for or at its own request, is left to the answer;
/// see `tauri/AGENTS.md`. One lock for both, so a close is never decided
/// between a request being answered and the count going down.
struct PendingPopup {
    requests: usize,
    close: bool,
}

static PENDING_POPUP: std::sync::Mutex<PendingPopup> = std::sync::Mutex::new(PendingPopup {
    requests: 0,
    close: false,
});

fn pending_popup() -> std::sync::MutexGuard<'static, PendingPopup> {
    PENDING_POPUP.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Closes the toolbar popup now, or once its requests are answered.
fn close_popup_when_free(app: &tauri::AppHandle) {
    {
        let mut pending = pending_popup();
        if pending.requests > 0 {
            pending.close = true;
            return;
        }
    }
    let app = app.clone();
    std::thread::spawn(move || webview::close_extension_popup(&app));
}

/// Records that one of the toolbar popup's requests has been answered, and
/// closes the popup if it opened something or was to close meanwhile.
fn popup_request_answered(app: &tauri::AppHandle, opened: bool) {
    let close = {
        let mut pending = pending_popup();
        pending.requests = pending.requests.saturating_sub(1);
        pending.close |= opened;
        let close = pending.requests == 0 && pending.close;
        if close {
            pending.close = false;
        }
        close
    };
    if close {
        let _ = webview::close_extension_popup(app);
    }
}

/// What an extension's toolbar popup reports: its requests to open a window,
/// answered as a page's are, and to close itself.
fn popup_sink(app: &tauri::AppHandle) -> platform::PageSink {
    let app = app.clone();
    Arc::new(move |signal| match signal {
        PageSignal::WindowRequested {
            request,
            url,
            popup,
            placement,
            background,
            window,
            gesture,
        } => {
            pending_popup().requests += 1;
            queue_window(
                &app,
                WindowRequest {
                    parent: None,
                    profile: Profile::Default,
                    session: 0,
                    slot: None,
                    request,
                    url,
                    popup,
                    placement,
                    background,
                    window,
                    gesture,
                },
            );
        }
        PageSignal::CloseRequested => close_popup_when_free(&app),
        _ => {}
    })
}

/// How long after a toolbar popup closed for losing focus a request to open
/// it again is taken as the click that closed it. Clicking the button of the
/// popup that is open closes it, as in other browsers, and the click reaches
/// the button only after the popup has lost focus to it.
const POPUP_TOGGLE: Duration = Duration::from_millis(400);

/// The toolbar popup open now, and the last one closed for losing focus,
/// with when.
#[derive(Default)]
struct PopupRecord {
    open: Option<String>,
    closed: Option<(String, std::time::Instant)>,
}

static POPUP: std::sync::Mutex<PopupRecord> = std::sync::Mutex::new(PopupRecord {
    open: None,
    closed: None,
});

/// Closes the toolbar popup when it loses focus, as other browsers do.
fn on_popup_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    // Focus a popup passes through while it is still hidden, being created
    // and sized, is not the user leaving it.
    if !matches!(event, tauri::WindowEvent::Focused(false)) || !window.is_visible().unwrap_or(false) {
        return;
    }
    if let Ok(mut popup) = POPUP.lock() {
        if let Some(url) = popup.open.take() {
            popup.closed = Some((url, std::time::Instant::now()));
        }
    }
    close_popup_when_free(window.app_handle());
}

/// Runs the shortcuts pressed while a window's interface has focus. Pages
/// report theirs through [`page_observer`], so both reach [`run_shortcut`].
///
/// A chord pressed while the window records one for settings goes to that
/// recording instead.
pub fn shortcut_sink(app: tauri::AppHandle, window: WindowId) -> KeySink {
    Arc::new(move |chord, repeat| {
        if shortcuts::record(window, chord, repeat) {
            return true;
        }
        let Some(shortcut) = keymap::shortcut_for(chord) else {
            return false;
        };
        if !repeat {
            queue_shortcut(&app, ShortcutRequest::chrome(window), shortcut);
        }
        true
    })
}

/// Runs shortcuts one at a time, in the order they were pressed.
///
/// A shortcut changes the browser and then applies the change to webviews,
/// and only the first half holds the browser's lock. Run side by side, two
/// quick Ctrl+Tabs could show their pages in the opposite order to the one
/// the browser recorded, leaving the selected tab showing another's page.
fn queue_shortcut(app: &tauri::AppHandle, request: ShortcutRequest, shortcut: Shortcut) {
    static QUEUE: OnceLock<mpsc::Sender<(ShortcutRequest, Shortcut)>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        let app = app.clone();
        std::thread::spawn(move || {
            for (request, shortcut) in receiver {
                run_shortcut(&app, request, shortcut);
            }
        });
        sender
    });
    let _ = queue.send((request, shortcut));
}

/// Revalidates ownership and dispatches without another change reusing its slot.
///
/// Off the UI thread, like a command: most shortcuts drive webviews. A failure
/// is shown in the window the shortcut was pressed in, as the button that does
/// the same would show it there.
fn run_shortcut(app: &tauri::AppHandle, request: ShortcutRequest, shortcut: Shortcut) {
    let state = app.state::<AppState>();
    if let Err(error) = run_shortcut_in(app, &state, request, shortcut) {
        report_failure(app, request.window(), error);
    }
}

fn run_shortcut_in(
    app: &tauri::AppHandle,
    state: &AppState,
    request: ShortcutRequest,
    shortcut: Shortcut,
) -> Result<()> {
    let home = home_url(state)?;
    let pressure = read_pressure(state);
    let native = std::cell::Cell::new((None, None, None, None, false));
    let failed = std::cell::Cell::new(None);
    state.changes.run(
        &state.browser,
        |browser| {
            if matches!(
                shortcut,
                Shortcut::ToggleSplit
                    | Shortcut::DuplicateTab
                    | Shortcut::Memory
                    | Shortcut::Home
                    | Shortcut::NewTab
                    | Shortcut::NewWindow
                    | Shortcut::NewPrivateWindow
                    | Shortcut::NextTab
                    | Shortcut::PreviousTab
                    | Shortcut::LastTab
            ) || shortcut.tab_index().is_some()
            {
                browser.set_pressure(pressure);
            }
            Ok(hold_then(state, browser, &failed, |browser| {
                let action = request.prepare(browser, shortcut, &home, now_ms())?;
                native.set((action.address, action.devtools, action.close, action.page, action.zoom));
                Ok(action.effects)
            }))
        },
        |effects| {
            if let Some(failure) = failed.take() {
                return commit_held(app, state, effects, failure);
            }
            // Committed even without native effects, as `mutate` is: tabs on
            // internal pages hold no webview, so opening, closing or moving
            // between them changes state and nothing else. Its failure is
            // returned after the native action, so a failure to watch a
            // private window's workers cannot stop Ctrl+F or Ctrl+L.
            let committed = commit(app, state, effects);
            let (address, devtools, close, page, zoom) = native.get();
            // Off this turn, which storing settings would wait on. The levels
            // are the browser's, which changed whether or not committing failed.
            if zoom {
                let app = app.clone();
                let window = request.window();
                std::thread::spawn(move || {
                    let _ = store_site_zoom(&app, &app.state::<AppState>(), window);
                });
            }
            committed.and(run_native(app, request, (address, devtools, close, page)))
        },
    )
}

/// What a shortcut does outside the browser's state, once that is committed.
fn run_native(
    app: &tauri::AppHandle,
    request: ShortcutRequest,
    (address, devtools, close, page): (Option<TabId>, Option<SlotId>, Option<WindowId>, Option<PageAction>),
) -> Result<()> {
    if let Some(window) = close.and_then(|id| app.get_window(&id.label())) {
        window.close()?;
    }
    if let Some(tab) = address {
        focus_address(app, tab)?;
    }
    if let Some(page) = devtools.and_then(|slot| app.get_webview(&slot.label())) {
        page.open_devtools();
    }
    match page {
        Some(PageAction::Find(tab)) => {
            // The page holds the keyboard; the bar's field can only
            // take it once the chrome has it, as for the address field.
            webview::chrome_of(app, request.window())?.set_focus()?;
            FindRequested(tab).emit_to(app, request.window().label())?;
        }
        Some(PageAction::Print(slot)) => print_slot(app, request.window(), slot)?,
        Some(PageAction::Save(slot)) => save_slot(app, request.window(), slot)?,
        None => {}
    }
    Ok(())
}

/// Opens the home page with `open`, in the address field, as every browser
/// starts a new tab or window.
fn open_home<F>(app: &tauri::AppHandle, state: &AppState, open: F) -> Result<()>
where
    F: FnOnce(&mut Browser, String) -> (TabId, Vec<Effect>),
{
    let home = home_url(state)?;
    let pressure = read_pressure(state);
    let mut opened = None;
    mutate(app, state, |browser| {
        browser.set_pressure(pressure);
        let (id, effects) = open(browser, home);
        opened = Some(id);
        Ok(effects)
    })?;
    opened.map_or(Ok(()), |id| focus_address(app, id))
}

/// Whether startup has finished, so a launch while Haku runs is answered.
static ANSWERING_LAUNCHES: AtomicBool = AtomicBool::new(false);

/// Starts answering launches, once the restored windows are ready.
pub fn answer_launches() {
    ANSWERING_LAUNCHES.store(true, Ordering::Release);
}

/// Answers Haku being launched while it already runs: the second launch has
/// exited, and this one opens a browser window on the home page, as Ctrl+N
/// does and as other browsers do when launched again.
///
/// A launch before startup has finished opens nothing, decided here as it
/// arrives: it is most often the same launch repeated while Haku is slow to
/// appear, and the windows starting up answer it. Called on the UI thread, so
/// windows are opened on a queue of their own, one launch after another, so
/// the window of the last launch is the one left in front.
pub fn launched_again(app: &tauri::AppHandle) {
    if !ANSWERING_LAUNCHES.load(Ordering::Acquire) {
        return;
    }
    static QUEUE: OnceLock<mpsc::Sender<()>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        let app = app.clone();
        std::thread::spawn(move || {
            let state = app.state::<AppState>();
            for () in receiver {
                // Read before opening, since the window that failed to open
                // is not one whose interface is listening.
                let Ok(from) = state.browser.read().map(|browser| browser.main_window()) else {
                    continue;
                };
                if let Err(error) = open_home(&app, &state, Browser::open_window) {
                    report_failure(&app, from, error);
                }
            }
        });
        sender
    });
    let _ = queue.send(());
}

/// Moves keyboard focus to the interface and asks it to focus the address
/// field for `tab`, which may not have rendered yet. A popup has no address
/// field, so its page keeps focus.
fn focus_address(app: &tauri::AppHandle, tab: TabId) -> Result<()> {
    let state = app.state::<AppState>();
    let (window, kind) = {
        let browser = state
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        let window = browser
            .window_of(tab)
            .ok_or_else(|| HakuError::TabNotFound(tab.0.to_string()))?;
        (window, browser.kind(window))
    };
    if kind == Some(WindowKind::Popup) {
        return Ok(());
    }
    webview::chrome_of(app, window)?.set_focus()?;
    AddressFocusRequested(tab)
        .emit_to(app, window.label())
        .map_err(HakuError::from)
}

fn home_url(state: &AppState) -> Result<String> {
    let settings = state
        .settings
        .read()
        .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
    Ok(settings.home_url.clone())
}

/// Switches installed extensions on or off where the settings changed them.
///
/// Waits for an add or a removal that is running, so it never switches an
/// extension while that replaces it, or that switches it from settings older
/// than these.
fn switch_extensions(app: &tauri::AppHandle, state: &AppState, before: &[String], after: &[String]) -> Result<()> {
    let _turn = super::extensions::lock(state);
    let installed: Vec<String> = state
        .extensions
        .read()
        .map_err(|_| HakuError::Storage("extensions lock poisoned".into()))?
        .installed
        .iter()
        .map(|extension| extension.id.clone())
        .collect();
    let changed = installed
        .into_iter()
        .filter(|id| before.contains(id) != after.contains(id));
    let mut chrome = None;
    for id in changed {
        let chrome = match &chrome {
            Some(chrome) => chrome,
            None => chrome.insert(webview::chrome(app)?),
        };
        let enabled = !after.contains(&id);
        platform::set_extension_enabled(chrome, id, enabled)?;
    }
    Ok(())
}

/// How often the tick runs while memory is plentiful.
const TICK: Duration = Duration::from_secs(5);

/// How often the tick runs under pressure, when memory runs out faster than
/// a slower timer would notice.
const TICK_PRESSED: Duration = Duration::from_secs(1);

/// Measures memory, reads running pages and applies the rule for as long as
/// the application does.
///
/// A thread of its own rather than an interface timer: low memory is a reason
/// to act whether or not anyone is looking at the window.
pub fn tick_periodically(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut interval = TICK;
        loop {
            std::thread::sleep(interval);
            let state = app.state::<AppState>();
            let pressure = measure_memory(&app, &state);
            interval = if pressure == Pressure::Normal {
                TICK
            } else {
                TICK_PRESSED
            };

            let readings = read_running_pages(&app, &state);
            let memory = match state.memory.read() {
                Ok(memory) => memory.slot_bytes(),
                Err(_) => continue,
            };
            let _ = state.changes.run(
                &state.browser,
                |browser| {
                    let mut effects = downloads::hold(&state, browser);
                    effects.extend(
                        readings
                            .into_iter()
                            .flat_map(|(tab, reading)| browser.report_state(tab, reading)),
                    );
                    effects.extend(browser.tick(now_ms(), pressure, memory));
                    Ok(effects)
                },
                // Most passes only re-time the visible tab, which is not worth
                // an event or a session write.
                |effects| {
                    if effects.is_empty() {
                        return Ok(());
                    }
                    commit(&app, &state, effects)
                },
            );
            if let Ok(report) = report_memory(&state) {
                let _ = MemoryChanged(report).emit(&app);
            }
        }
    });
}

/// Takes a new memory reading into the state.
///
/// @returns The pressure level the reading puts the machine at.
fn measure_memory(app: &tauri::AppHandle, state: &AppState) -> Pressure {
    let slots: Vec<(SlotId, tauri::Webview)> = match state.browser.read() {
        Ok(browser) => browser
            .slot_ids()
            .into_iter()
            .filter_map(|slot| app.get_webview(&slot.label()).map(|webview| (slot, webview)))
            .collect(),
        Err(_) => Vec::new(),
    };
    let attribution = webview::chrome(app)
        .and_then(|chrome| platform::slot_memory(&chrome, &slots))
        .ok();
    let status = platform::memory_status();

    let Ok(mut memory) = state.memory.write() else {
        return Pressure::Normal;
    };
    *memory = memory.next(status, attribution);
    memory.pressure
}

/// Reads the pressure level again without measuring slots, which is cheap
/// enough to do before every tab switch.
fn read_pressure(state: &AppState) -> Pressure {
    let status = platform::memory_status();
    let Ok(mut memory) = state.memory.write() else {
        return Pressure::Normal;
    };
    *memory = memory.next(status, None);
    memory.pressure
}

fn report_memory(state: &AppState) -> Result<MemoryReport> {
    let memory = state
        .memory
        .read()
        .map_err(|_| HakuError::Storage("memory lock poisoned".into()))?;
    let browser = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    Ok(memory.report(&browser))
}

fn record_page(app: &tauri::AppHandle, slot: SlotId, commits: &[crate::model::Commit], title: Option<String>) {
    let Some(handle) = app.try_state::<AppState>() else {
        return;
    };
    // `inner` re-borrows from the app rather than the local handle, so the lock
    // guards below can outlive it.
    let state: &AppState = handle.inner();

    let (recorded, stale) = {
        let Ok(mut browser) = state.browser.write() else {
            return;
        };
        let recorded = browser.report_page(slot, commits, title);
        (recorded, browser.has_stale_permissions(slot))
    };

    // A request from an origin the page has left is answered off this thread,
    // which answering dispatches to.
    if stale {
        let app = app.clone();
        std::thread::spawn(move || {
            let state = app.state::<AppState>();
            let _ = mutate(&app, &state, |browser| Ok(browser.withdraw_stale_permissions(slot)));
        });
    }

    if let Some(report) = recorded {
        queue_record(app, report);
    }

    let states = state.browser.read().map(|browser| window_states(&browser));
    if let Ok(states) = states {
        emit_states(app, states);
    }
}

/// What each window shows, read under the lock and sent after it is released.
fn window_states(browser: &Browser) -> Vec<(WindowId, BrowserState)> {
    browser
        .window_ids()
        .into_iter()
        .filter_map(|window| Some((window, browser.state_in(window)?)))
        .collect()
}

/// Sends each window's interface what it shows.
///
/// Each chrome listens for its own webview only: a listener for any target
/// would also receive every other window's state.
fn emit_states(app: &tauri::AppHandle, states: Vec<(WindowId, BrowserState)>) {
    for (window, snapshot) in states {
        let _ = StateChanged(snapshot).emit_to(app, window.label());
    }
}

/// Carries out a webview's own back or forward through the tab's history.
fn traverse(app: &tauri::AppHandle, slot: SlotId, url: &str) {
    let state = app.state::<AppState>();
    let _ = mutate(app, &state, |browser| match browser.traversal(slot, url) {
        Some((id, Direction::Back)) => browser.go_back(id),
        Some((id, Direction::Forward)) => browser.go_forward(id),
        None => Ok(Vec::new()),
    });
}

/// Resolves explicit focus and repairs failed bindings within the change turn.
fn apply_effects(app: &tauri::AppHandle, state: &AppState, effects: &[Effect]) -> Result<(Vec<Departure>, Result<()>)> {
    let (effects, placements, bounds) = {
        let mut browser = state
            .browser
            .write()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        (
            browser.resolve_pane_focus(effects, |target| state.viewports.can_show(target)),
            browser.slot_viewports(),
            state.viewports.all_slot_bounds(&browser),
        )
    };
    let hooks = hooks();
    for effect in &effects {
        if let Effect::Destroy { slot } = effect {
            menu_owners().retain(|_, (owner, _)| owner != slot);
            downloads::slot_gone(app, state, *slot);
        }
        ask_to_close(app, effect);
    }
    let (mut departures, failed_slots, mut applied) =
        webview::apply(app, &effects, &state.viewports, &hooks, &placements, &bounds);
    let cleanup: Vec<Effect> = {
        let mut browser = state
            .browser
            .write()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        let mut cleanup = browser.slots_failed(failed_slots.iter().copied());
        cleanup.extend(browser.slots_created(effects.iter().filter_map(|effect| match effect {
            Effect::EnsureSlot { slot, .. } | Effect::Adopt { slot, .. } if !failed_slots.contains(slot) => Some(*slot),
            _ => None,
        })));
        cleanup
    };
    if !cleanup.is_empty() {
        let (more, cleaned) = apply_effects(app, state, &cleanup)?;
        departures.extend(more);
        applied = applied.and(cleaned);
    }
    Ok((departures, applied))
}

/// Asks a window to confirm a close that would cancel downloads.
fn ask_to_close(app: &tauri::AppHandle, effect: &Effect) {
    if let Effect::ConfirmClose {
        window,
        tab,
        ending,
        downloads,
    } = *effect
    {
        let _ = CloseConfirmationRequested { tab, ending, downloads }.emit_to(app, window.label());
    }
}

/// Applies effects to real webviews and tells every window what changed.
///
/// What the browser decided is shown and saved even when an effect failed,
/// since the browser already holds it; the first failure is returned after.
fn commit(app: &tauri::AppHandle, state: &AppState, effects: &[Effect]) -> Result<()> {
    let (departures, mut applied) = apply_effects(app, state, effects)?;
    let mut readings = keep_previews(state, departures);
    // A page read as it was left may turn out to be capturing, which changes
    // what should happen to it. Bounded, though a reading only ever leads to
    // freezing or resuming one tab.
    for _ in 0..MAX_READING_ROUNDS {
        if readings.is_empty() {
            break;
        }
        let effects = report_readings(state, readings)?;
        let (departures, more) = apply_effects(app, state, &effects)?;
        readings = keep_previews(state, departures);
        applied = applied.and(more);
    }

    let states = {
        let browser = state
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        // A closed tab's capture shows a page nobody can return to.
        if let Ok(mut previews) = state.previews.lock() {
            previews.retain(|id| browser.tabs().iter().any(|tab| tab.id == id));
        }
        if !browser.has_private_tabs() {
            downloads::end_private_session(app, state);
        }
        window_states(&browser)
    };
    emit_states(app, states);
    let _ = state.save_session();
    applied
}

/// Stores the captures taken as pages were left, and passes on what each
/// page held.
fn keep_previews(state: &AppState, departures: Vec<Departure>) -> Vec<PageReading> {
    let Ok(mut previews) = state.previews.lock() else {
        return departures.into_iter().map(|left| (left.tab, left.state)).collect();
    };
    departures
        .into_iter()
        .map(|left| {
            if let Some(jpeg) = left.preview {
                previews.insert(left.tab, jpeg);
            }
            (left.tab, left.state)
        })
        .collect()
}

/// More rounds of reading than one tab switch can cause.
const MAX_READING_ROUNDS: usize = 4;

fn report_readings(state: &AppState, readings: Vec<PageReading>) -> Result<Vec<Effect>> {
    let mut browser = state
        .browser
        .write()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    Ok(readings
        .into_iter()
        .flat_map(|(tab, reading)| browser.report_state(tab, reading))
        .collect())
}

/// Reads every running page: the visible one, so a page that hangs as it is
/// left still has a recent reading, and background ones, which is how the end
/// of a capture is noticed. Frozen pages are not read.
fn read_running_pages(app: &tauri::AppHandle, state: &AppState) -> Vec<PageReading> {
    let pages = match state.browser.read() {
        Ok(browser) => browser.running_pages(),
        Err(_) => return Vec::new(),
    };
    pages
        .into_iter()
        .map(|(slot, tab)| (tab, webview::read_page(app, slot, webview::LEAVE_TIMEOUT)))
        .collect()
}

/// Runs `mutate` under the write lock and commits whatever it produced.
///
/// The lock is released before webviews are touched, so a native call that
/// blocks cannot stall a read; the next change waits for this one to be
/// applied, through [`crate::state::Changes`].
pub(super) fn mutate<F>(app: &tauri::AppHandle, state: &AppState, mutate: F) -> Result<()>
where
    F: FnOnce(&mut Browser) -> Result<Vec<Effect>>,
{
    let failed = std::cell::Cell::new(None);
    state.changes.run(
        &state.browser,
        |browser| Ok(hold_then(state, browser, &failed, mutate)),
        |effects| match failed.take() {
            Some(failure) => commit_held(app, state, effects, failure),
            None => commit(app, state, effects),
        },
    )
}

/// Holds the webviews downloads run in, then runs `change`.
///
/// A failed change is set in `failed` rather than returned, so the hold's
/// effects survive it: the pool has already let go of the webviews they
/// destroy, and dropping them would leak those webviews.
fn hold_then<F>(
    state: &AppState,
    browser: &mut Browser,
    failed: &std::cell::Cell<Option<HakuError>>,
    change: F,
) -> Vec<Effect>
where
    F: FnOnce(&mut Browser) -> Result<Vec<Effect>>,
{
    let mut effects = downloads::hold(state, browser);
    match change(browser) {
        Ok(more) => effects.extend(more),
        Err(failure) => failed.set(Some(failure)),
    }
    effects
}

/// Commits what [`hold_then`] held before its change failed.
///
/// @returns The change's failure, which is what its caller asked about, in
///   place of any failure applying the hold.
fn commit_held(app: &tauri::AppHandle, state: &AppState, effects: &[Effect], failure: HakuError) -> Result<()> {
    if !effects.is_empty() {
        let _ = commit(app, state, effects);
    }
    Err(failure)
}

/// [`mutate`], for a command a window's interface called.
///
/// @returns What that window shows now, or what the normal window used last
///   shows when the change closed it.
fn mutate_in<F>(app: &tauri::AppHandle, state: &AppState, window: WindowId, change: F) -> Result<BrowserState>
where
    F: FnOnce(&mut Browser) -> Result<Vec<Effect>>,
{
    mutate(app, state, |browser| {
        if browser.kind(window).is_none() {
            return Err(HakuError::WindowMissing(window.label()));
        }
        change(browser)
    })?;
    let browser = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    Ok(browser.state_in(window).unwrap_or_else(|| browser.state()))
}

/// Follows what happens to a browser window that only the window system
/// reports: being closed, used, moved or resized.
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if webview::is_extension_popup(window.label()) {
        on_popup_event(window, event);
        return;
    }
    let Some(id) = WindowId::from_label(window.label()) else {
        return;
    };
    let state = window.state::<AppState>();
    match event {
        // Closed by the browser rather than by the window system, so its tabs
        // and webviews go with it, and the last window is kept for the next
        // launch rather than emptied.
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let app = window.app_handle().clone();
            std::thread::spawn(move || close_window(&app, id));
        }
        tauri::WindowEvent::Focused(true) => {
            if let Ok(mut browser) = state.browser.write() {
                browser.focus(id);
            }
        }
        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => record_bounds(window, id, &state),
        tauri::WindowEvent::Destroyed => shortcuts::cancel_recording(id),
        _ => {}
    }
}

/// Closes a browser window, or quits when it is the last one. A close that
/// would cancel downloads asks the window to confirm it instead.
///
/// Off the UI thread: closing drives webviews.
fn close_window(app: &tauri::AppHandle, window: WindowId) {
    let state = app.state::<AppState>();
    let (last, held) = state.browser.read().map_or((true, None), |browser| {
        (browser.is_last_window(window), browser.close_held(window, None))
    });
    if let Some(held) = held {
        ask_to_close(app, &held);
        return;
    }
    if last {
        // The session already holds the window's tabs; only where it was may
        // have changed since it was last saved.
        let _ = state.save_session();
        app.exit(0);
        return;
    }
    let _ = mutate(app, &state, |browser| Ok(browser.close_window(window)));
    if let Ok(mut bounds) = state.bounds.write() {
        bounds.remove(&window);
    };
}

/// Remembers where a window is on screen, for the session. A minimised
/// window is remembered where it was before, and a maximised one keeps the
/// rectangle it returns to. A window maximised before it ever had another
/// rectangle is not remembered, so it opens maximized again. A fullscreen
/// window is remembered as it was before, since it fills the screen only
/// for the page that asked.
fn record_bounds(window: &tauri::Window, id: WindowId, state: &AppState) {
    if window.is_minimized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }
    let maximized = window.is_maximized().unwrap_or(false);
    let Ok(mut bounds) = state.bounds.write() else { return };
    if maximized {
        if let Some(known) = bounds.get_mut(&id) {
            known.maximized = true;
        }
        return;
    }
    let (Ok(scale), Ok(position), Ok(size)) = (window.scale_factor(), window.outer_position(), window.inner_size())
    else {
        return;
    };
    let (position, size) = (position.to_logical::<f64>(scale), size.to_logical::<f64>(scale));
    bounds.insert(
        id,
        WindowBounds {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
            maximized,
        },
    );
}

#[tauri::command]
#[specta::specta]
pub fn get_state(webview: tauri::Webview, state: State<'_, AppState>) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    // The interface asks for its state once its listeners are attached, so
    // this is the first moment a failure sent to it is not lost. A window that
    // is closing leaves them held for the next one: the browser stays locked
    // from the check to the take, so the window cannot close between them.
    let (browser_state, failures) = {
        let browser = state
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        let browser_state = browser
            .state_in(window)
            .ok_or_else(|| HakuError::WindowMissing(window.label()))?;
        (browser_state, state.held_failures.take())
    };
    for failure in failures {
        let _ = FailureReported(failure).emit_to(&webview, webview.label());
    }
    Ok(browser_state)
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(webview: tauri::Webview, state: State<'_, AppState>) -> Result<Settings> {
    ensure_chrome(&webview)?;
    let settings = state
        .settings
        .read()
        .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
    Ok(settings.clone())
}

/// Replaces the settings, applying anything that takes effect immediately.
///
/// Settings are shared: whichever window calls this changes them for every
/// window and the one pool. Every window's interface hears of the change once
/// it has been stored, before it is applied. Stored settings stand when
/// applying them fails partway: the failure is returned to the caller alone,
/// and they are applied again at the next start.
///
/// Lowering the webview capacity destroys surplus webviews right away rather
/// than waiting for the next tab switch, because the point of lowering it is to
/// release memory now.
#[tauri::command]
#[specta::specta]
pub async fn set_settings(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings> {
    let window = ensure_chrome(&webview)?;
    let previous = state
        .settings
        .read()
        .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?
        .site_zoom
        .clone();
    let stored = store_settings(&app, &state, window, |_| settings)?;
    // Only the settings page changes stored levels this way; a level the user
    // zooms to is the browser's already.
    if stored.site_zoom != previous {
        mutate(&app, &state, |browser| {
            Ok(browser.change_site_zoom(&previous, &stored.site_zoom))
        })?;
    }
    Ok(stored)
}

/// Replaces the optimization settings with a preset's values.
///
/// Like any settings change, it applies to every window, not only the caller.
///
/// Resolved here so the preset values have one definition, in Rust.
#[tauri::command]
#[specta::specta]
pub async fn apply_preset(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    preset: Preset,
) -> Result<Settings> {
    let window = ensure_chrome(&webview)?;
    store_settings(&app, &state, window, |current| current.clone().with_preset(preset))
}

/// The preset the current settings match, or nothing when they were customised.
#[tauri::command]
#[specta::specta]
pub fn current_preset(webview: tauri::Webview, state: State<'_, AppState>) -> Result<Option<Preset>> {
    ensure_chrome(&webview)?;
    let settings = state
        .settings
        .read()
        .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
    Ok(settings.preset())
}

/// Every shortcut with its chords now and its defaults, in table order.
/// [Shortcuts § Changing bindings](../../../docs/specs/shortcuts.md#changing-bindings).
#[tauri::command]
#[specta::specta]
pub fn keymap(webview: tauri::Webview) -> Result<Vec<KeyBinding>> {
    ensure_chrome(&webview)?;
    Ok(key_bindings(&keymap::active()))
}

/// The next chord pressed in the caller's window, for settings to bind, or
/// nothing when the recording is cancelled: by plain Esc, by
/// [`cancel_shortcut_recording`], or by another recording in the window.
///
/// Until it answers, the window's interface runs no shortcut.
#[tauri::command]
#[specta::specta]
pub async fn record_shortcut(webview: tauri::Webview) -> Result<Option<RecordedChord>> {
    let window = ensure_chrome(&webview)?;
    let mut answer = shortcuts::start_recording(window);
    Ok(answer.recv().await.flatten().map(RecordedChord::from))
}

/// Ends the recording pending in the caller's window, if any.
#[tauri::command]
#[specta::specta]
pub fn cancel_shortcut_recording(webview: tauri::Webview) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    shortcuts::cancel_recording(window);
    Ok(())
}

/// Stores the settings `update` makes from the current ones. It runs within
/// the store's turn, so a change made from the current settings cannot undo an
/// overlapping store.
fn store_settings(
    app: &tauri::AppHandle,
    state: &AppState,
    window: WindowId,
    update: impl FnOnce(&Settings) -> Settings,
) -> Result<Settings> {
    // One store at a time, from replacing through applying, so a failed save
    // never restores settings older than a store that overlapped, and the
    // browser runs with the settings stored last (issue #41). Held across the
    // main thread driving webviews; that is safe because no main-thread path
    // takes it and both callers are async.
    static STORING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let storing = STORING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let (previous, settings) = {
        let mut current = state
            .settings
            .write()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        let settings = update(&current).sanitized();
        if current.keeps_new_site(&settings)
            || current.adds_site_permission(&settings)
            || current.adds_site_zoom(&settings)
        {
            state
                .browser
                .read()
                .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?
                .allow_site_settings(window)?;
        }
        (std::mem::replace(&mut *current, settings.clone()), settings)
    };
    if let Err(error) = state.save_settings() {
        let mut current = state
            .settings
            .write()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        *current = previous;
        return Err(error);
    }
    // Announced once stored, before applying, so every window shows the stored
    // settings even when applying them fails. Those stand and are applied again
    // at the next start; only the caller hears of the failure. Announced within
    // the store's turn, so windows hear overlapping stores in the order they
    // were stored. A failed announcement is reported after applying, not
    // instead of it.
    // In effect before it is announced, so a window reading the keymap on
    // hearing of the change reads the new one.
    keymap::set_active(settings.shortcuts.keymap());
    let announced = SettingsChanged(settings.clone()).emit(app);

    let (capacity, freeze, discard) = (settings.pool_capacity(), settings.freeze_tabs, settings.discard_tabs);
    let (budget, kept_sites) = (settings.kept_memory_bytes(), settings.kept_sites.clone());
    let site_permissions = settings.site_permissions.to_vec();
    mutate(app, state, |browser| {
        browser.set_keeping(budget, kept_sites);
        browser.set_site_permissions(site_permissions);
        Ok(browser.set_optimization(capacity, freeze, discard))
    })?;
    switch_extensions(app, state, &previous.disabled_extensions, &settings.disabled_extensions)?;
    drop(storing);
    announced.map_err(HakuError::from)?;
    Ok(settings)
}

#[tauri::command]
#[specta::specta]
pub async fn open_tab(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    url: Option<String>,
    activate: bool,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    let target = match url {
        Some(url) => {
            let settings = state
                .settings
                .read()
                .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
            resolve_target(&url, &settings.search_url)
        }
        None => {
            let settings = state
                .settings
                .read()
                .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
            settings.home_url.clone()
        }
    };

    let pressure = read_pressure(&state);
    mutate_in(&app, &state, window, |browser| {
        browser.set_pressure(pressure);
        let (_, effects) = browser.open_tab_in(window, target, activate);
        Ok(effects)
    })
}

#[tauri::command]
#[specta::specta]
pub async fn close_tab(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    let home = {
        let settings = state
            .settings
            .read()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        settings.home_url.clone()
    };
    mutate_in(&app, &state, window, |browser| browser.close_tab(id, &home))
}

#[tauri::command]
#[specta::specta]
pub async fn select_tab(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    let pressure = read_pressure(&state);
    mutate_in(&app, &state, window, |browser| {
        browser.set_pressure(pressure);
        browser.select_tab(id, now_ms())
    })
}

/// A numeric IPC argument; specta otherwise exports f64 as number | null.
#[derive(serde::Deserialize, specta::Type)]
#[serde(transparent)]
#[specta(inline)]
pub struct SplitRatio(#[specta(type = specta_typescript::Number)] f64);

/// Pairs a distinct unpaired tab with the selected unpaired tab.
/// # Errors
/// Rejects non-chrome callers and invalid model targets; propagates effect failures.
#[tauri::command]
#[specta::specta]
pub async fn split_tab(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    let current = mutate_in(&app, &state, window, |browser| browser.split_tab(window, tab))?;
    if current
        .tabs
        .iter()
        .any(|entry| entry.id == tab && entry.presence == crate::model::TabPresence::Discarded)
    {
        return Err(HakuError::NoSlotAvailable);
    }
    Ok(current)
}

/// Separates the named tab's pair, retaining both tabs.
/// # Errors
/// Rejects non-chrome callers and invalid model targets; propagates effect failures.
#[tauri::command]
#[specta::specta]
pub async fn separate_tabs(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    mutate_in(&app, &state, window, |browser| browser.separate_tabs(window, tab))
}

/// Commits the ratio of the named tab's pair, including background pairs.
/// # Errors
/// Rejects non-chrome callers and invalid model targets; propagates effect failures.
#[tauri::command]
#[specta::specta]
pub async fn set_split_ratio(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
    ratio: SplitRatio,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    mutate_in(&app, &state, window, |browser| {
        browser.set_split_ratio(window, tab, ratio.0)
    })
}

/// Focuses the named visible pane.
/// # Errors
/// Rejects non-chrome callers and invalid model targets; propagates effect failures.
#[tauri::command]
#[specta::specta]
pub async fn focus_pane(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    mutate_in(&app, &state, window, |browser| browser.focus_pane(window, tab))
}

/// Navigates a tab to whatever the user typed.
///
/// The input is resolved here rather than in the interface so that the same
/// rules apply to every entry point, including future ones like shortcuts.
#[tauri::command]
#[specta::specta]
pub async fn navigate_tab(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
    input: String,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    let target = {
        let settings = state
            .settings
            .read()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        resolve_target(&input, &settings.search_url)
    };
    if target.is_empty() {
        return get_state(webview, state);
    }
    mutate_in(&app, &state, window, |browser| browser.navigate(id, target))
}

#[tauri::command]
#[specta::specta]
pub async fn go_back(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    mutate_in(&app, &state, window, |browser| browser.go_back(id))
}

#[tauri::command]
#[specta::specta]
pub async fn go_forward(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    mutate_in(&app, &state, window, |browser| browser.go_forward(id))
}

#[tauri::command]
#[specta::specta]
pub async fn reload_tab(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    mutate_in(&app, &state, window, |browser| browser.reload(id))
}

/// Opens a browser window whose one tab loads `url`, as Shift+clicking a link
/// does. `url` is resolved as the address field resolves it.
#[tauri::command]
#[specta::specta]
pub async fn open_window(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    url: String,
    is_private: Option<bool>,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    let target = {
        let settings = state
            .settings
            .read()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        resolve_target(&url, &settings.search_url)
    };
    let pressure = read_pressure(&state);
    mutate_in(&app, &state, window, |browser| {
        browser.set_pressure(pressure);
        let private = is_private.unwrap_or_else(|| browser.is_private(window));
        Ok(if private {
            browser.open_private_window(target).1
        } else {
            browser.open_window(target).1
        })
    })
}

/// Reopens the most recently closed tab, as Ctrl+Shift+T does. Acts on the
/// whole browser: the tab goes back to the window it was closed in.
#[tauri::command]
#[specta::specta]
pub async fn reopen_closed_tab(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    mutate_in(&app, &state, window, |browser| Ok(browser.reopen_closed_tab()))
}

#[tauri::command]
#[specta::specta]
pub async fn set_tab_fixed(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
    fixed: bool,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    mutate_in(&app, &state, window, |browser| browser.set_fixed(id, fixed))
}

#[tauri::command]
#[specta::specta]
pub async fn reorder_tab(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
    to: u32,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    mutate_in(&app, &state, window, |browser| {
        browser.reorder_tab(id, to as usize)?;
        Ok(Vec::new())
    })
}

/// Tells Haku where the interface is and what it occupies.
///
/// This one command carries the viewport and the interactive regions together
/// so the content webview bounds and the chrome input mask can never disagree,
/// which is what would otherwise show as a flicker at the edge of the page.
#[tauri::command]
#[specta::specta]
pub async fn set_layout(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    layout: Layout,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;

    let accepted = std::cell::Cell::new(false);
    let failed = std::cell::Cell::new(None);
    state.changes.run(
        &state.browser,
        |browser| {
            Ok(hold_then(&state, browser, &failed, |browser| {
                accepted.set(state.set_layout(browser, window, &layout)?);
                Ok(if accepted.get() {
                    browser.reconcile()
                } else {
                    Vec::new()
                })
            }))
        },
        |effects| {
            if let Some(failure) = failed.take() {
                return commit_held(&app, &state, effects, failure);
            }
            if accepted.get() {
                // Bounds, mask and reconciliation share the serialized change turn.
                let bounds = {
                    let browser = state
                        .browser
                        .read()
                        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
                    state.viewports.slot_bounds(&browser, window)
                };
                return apply_layout_turn(
                    bounds
                        .into_iter()
                        .map(|(slot, rect)| webview::set_bounds(&app, slot, rect)),
                    || chrome::apply_layout(&webview, &layout, webview.window().scale_factor()?),
                    || commit(&app, &state, effects),
                );
            }
            // A stale layout changes nothing, but a download that ended may
            // have trimmed the pool, and its webviews still have to go.
            if effects.is_empty() {
                return Ok(());
            }
            commit(&app, &state, effects)
        },
    )?;
    let browser = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    Ok(browser.state_in(window).unwrap_or_else(|| browser.state()))
}

// Once the model has changed, bounds, mask and commit must all be attempted.
fn apply_layout_turn(
    bounds: impl Iterator<Item = Result<()>>,
    mask: impl FnOnce() -> Result<()>,
    commit: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let mut applied = Ok(());
    for result in bounds {
        applied = applied.and(result);
    }
    applied.and(mask()).and(commit())
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn only_the_newest_fullscreen_report_of_a_slot_applies() {
        let mut reports = FullscreenReports::default();
        let (slot, other) = (SlotId(1), SlotId(2));
        let enter = reports.next(slot);
        let exit = reports.next(slot);
        let elsewhere = reports.next(other);

        // An enter whose thread runs after the exit's changes nothing.
        assert!(!reports.is_latest(slot, enter));
        assert!(reports.is_latest(slot, exit));
        assert!(reports.is_latest(other, elsewhere));
    }
    use std::cell::RefCell;

    #[test]
    fn a_failed_pane_bound_does_not_skip_siblings_mask_or_commit() {
        let attempted = RefCell::new(Vec::new());
        let result = apply_layout_turn(
            (0..2).map(|pane| {
                attempted.borrow_mut().push(pane);
                Err(HakuError::Tauri(format!("pane {pane}")))
            }),
            || {
                attempted.borrow_mut().push(2);
                Err(HakuError::Tauri("mask".into()))
            },
            || {
                attempted.borrow_mut().push(3);
                Ok(())
            },
        );
        assert_eq!(*attempted.borrow(), [0, 1, 2, 3]);
        assert!(matches!(result, Err(HakuError::Tauri(message)) if message == "pane 0"));
    }

    #[test]
    fn a_failed_mask_still_commits_the_model_and_reports_the_first_error() {
        let committed = std::cell::Cell::new(false);
        let result = apply_layout_turn(
            std::iter::once(Ok(())),
            || Err(HakuError::Tauri("mask".into())),
            || {
                committed.set(true);
                Err(HakuError::Tauri("commit".into()))
            },
        );
        assert!(committed.get());
        assert!(matches!(result, Err(HakuError::Tauri(message)) if message == "mask"));
    }
}

#[cfg(test)]
mod hold_tests {
    use super::*;
    use crate::model::{Downloading, Policy, Pressure};
    use crate::storage::{HistoryDb, Paths, Settings};

    #[test]
    fn a_failed_change_keeps_what_the_download_hold_decided() {
        // A webview the pool kept only for a download whose end the browser
        // has not heard of yet.
        let mut browser = Browser::new(2);
        let (tab, _) = browser.open_tab("https://a.test", true);
        browser.open_tab("https://b.test", true);
        let slot = browser.tab(tab).unwrap().slot().unwrap();
        browser.set_downloading(Downloading {
            slots: std::collections::HashSet::from([slot]),
            ordinary: 1,
            private: 0,
        });
        browser.set_pressure(Pressure::Tight);
        browser.set_optimization(1, Policy::Always, Policy::Always);
        let state = AppState::new(
            browser,
            Settings::default(),
            HistoryDb::open_or_in_memory(std::path::Path::new(":memory:")).0,
            Paths::under(&std::env::temp_dir().join("haku-hold-tests")),
        );
        let failed = std::cell::Cell::new(None);

        let effects = {
            let mut browser = state.browser.write().unwrap();
            hold_then(&state, &mut browser, &failed, |_| {
                Err(HakuError::Tauri("change".into()))
            })
        };

        assert!(effects.contains(&Effect::Destroy { slot }));
        assert!(matches!(failed.take(), Some(HakuError::Tauri(message)) if message == "change"));
    }
}

/// Records a visit, or retitles the last one, tolerating a failure rather than
/// breaking navigation.
/// Writes what a page change leaves on disk, the visit and the session, off
/// the UI thread and in the order the pages changed.
///
/// Page reports arrive on the UI thread, and both writes wait on the disk: the
/// session file is synced, and the history file may be held by another Haku.
/// The session is written once for every report that queued up meanwhile, so
/// a page pushing routes quickly costs one write, not one per route; it
/// should reopen where the page actually is, not where it was opened.
fn queue_record(app: &tauri::AppHandle, report: PageReport) {
    static QUEUE: OnceLock<mpsc::Sender<PageReport>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<PageReport>();
        let app = app.clone();
        std::thread::spawn(move || {
            let state = app.state::<AppState>();
            while let Ok(report) = receiver.recv() {
                record_visit(&state, &report);
                for report in receiver.try_iter() {
                    record_visit(&state, &report);
                }
                let _ = state.save_session();
            }
        });
        sender
    });
    let _ = queue.send(report);
}

fn record_visit(state: &AppState, report: &PageReport) {
    let Ok(history) = state.history.lock() else { return };
    let _ = if report.visited {
        history.record(&report.url, &report.title, now_ms() as i64)
    } else {
        history.retitle(&report.url, &report.title)
    };
}

/// What `haku://memory` shows: the last tick's reading against the slots as
/// they are now.
#[tauri::command]
#[specta::specta]
pub fn memory_report(webview: tauri::Webview, state: State<'_, AppState>) -> Result<MemoryReport> {
    ensure_chrome(&webview)?;
    report_memory(&state)
}

/// What a tab showed as it was last left, as a `data:` URL, to cover its page
/// while it reloads. Nothing when no capture is held.
///
/// A command rather than a protocol: a protocol would be reachable from
/// content webviews, and a capture shows another tab's page.
#[tauri::command]
#[specta::specta]
pub fn tab_preview(webview: tauri::Webview, state: State<'_, AppState>, id: TabId) -> Result<Option<String>> {
    ensure_chrome(&webview)?;
    let previews = state
        .previews
        .lock()
        .map_err(|_| HakuError::Storage("preview lock poisoned".into()))?;
    Ok(previews.get(id).map(jpeg_data_url))
}

#[tauri::command]
#[specta::specta]
pub fn recent_history(webview: tauri::Webview, state: State<'_, AppState>, limit: u32) -> Result<Vec<HistoryEntry>> {
    ensure_chrome(&webview)?;
    let history = state
        .history
        .lock()
        .map_err(|_| HakuError::Storage("history lock poisoned".into()))?;
    history.recent(limit as usize)
}

#[tauri::command]
#[specta::specta]
pub fn clear_history(webview: tauri::Webview, state: State<'_, AppState>) -> Result<()> {
    ensure_chrome(&webview)?;
    let history = state
        .history
        .lock()
        .map_err(|_| HakuError::Storage("history lock poisoned".into()))?;
    history.clear()
}

/// Answers the context menu a page is waiting on, with the native item
/// chosen, or with nothing when the choice was one of Haku's own items or the
/// menu was dismissed. An answer to an unknown or already answered request is
/// ignored, because a menu can close from several causes at once.
///
/// The page gets keyboard focus back for a native item, and when `refocus`
/// asks for it: the menu closed from inside with nothing chosen.
#[tauri::command]
#[specta::specta]
pub async fn answer_context_menu(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    request: ContextMenuRequestId,
    choice: Option<i32>,
    refocus: bool,
) -> Result<()> {
    ensure_chrome(&webview)?;
    let Some((slot, tab)) = menu_owners().remove(&request) else {
        return Ok(());
    };
    let still_there = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?
        .tab_in(slot)
        == Some(tab);
    let choice = choice.filter(|_| still_there);
    // A native item acts on the page, which has to have focus for it, as it
    // had when it was right-clicked.
    if still_there && (choice.is_some() || refocus) {
        if let Some(page) = app.get_webview(&slot.label()) {
            let _ = page.set_focus();
        }
    }
    platform::answer_context_menu(&app, request, choice)
}

/// Answers the permission request a tab is showing, storing an Allow or a
/// Block made in an ordinary window. [Permissions](../../../docs/specs/permissions.md).
#[tauri::command]
#[specta::specta]
pub async fn answer_permission(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
    request: PermissionRequestId,
    decision: PermissionDecision,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    let mut decided = None;
    let snapshot = mutate_in(&app, &state, window, |browser| {
        if browser.window_of(tab) != Some(window) {
            return Err(HakuError::TabNotFound(tab.0.to_string()));
        }
        let (effects, store) = browser.answer_permission(tab, request, decision)?;
        decided = store;
        Ok(effects)
    })?;
    let Some(decided) = decided else {
        return Ok(snapshot);
    };
    store_settings(&app, &state, window, |current| {
        let mut settings = current.clone();
        settings
            .site_permissions
            .retain(|other| other.origin != decided.origin || other.kind != decided.kind);
        settings.site_permissions.push(decided);
        settings
    })?;
    let browser = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    Ok(browser.state_in(window).unwrap_or_else(|| browser.state()))
}

/// Returns the tab's site to 100%, as Ctrl+0 does. [Zoom](../../../docs/specs/zoom.md).
#[tauri::command]
#[specta::specta]
pub async fn reset_zoom(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    let mut stored = false;
    let snapshot = mutate_in(&app, &state, window, |browser| {
        browser.visible_tab_in(window, tab)?;
        let (effects, changed) = browser.reset_zoom(tab)?;
        stored = changed;
        Ok(effects)
    })?;
    if !stored {
        return Ok(snapshot);
    }
    store_site_zoom(&app, &state, window)?;
    let browser = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    Ok(browser.state_in(window).unwrap_or_else(|| browser.state()))
}

/// Answers the dialog a page is paused on.
#[tauri::command]
#[specta::specta]
pub async fn answer_dialog(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
    dialog: DialogId,
    answer: DialogAnswer,
) -> Result<BrowserState> {
    let window = ensure_chrome(&webview)?;
    mutate_in(&app, &state, window, |browser| {
        browser.answer_dialog(tab, dialog, answer)
    })
}

#[tauri::command]
#[specta::specta]
pub async fn open_tab_devtools(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
) -> Result<()> {
    ensure_chrome(&webview)?;
    let slot = {
        let browser = state
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        browser.tab(id)?.slot()
    };
    if let Some(target) = slot.and_then(|slot| app.get_webview(&slot.label())) {
        target.open_devtools();
    }
    Ok(())
}

/// The slot of a tab's live page in the calling `window`, or nothing for a
/// tab without one. A tab in another window is not found, as for any command
/// that acts on a window.
fn slot_of(state: &AppState, window: WindowId, id: TabId) -> Result<Option<SlotId>> {
    let browser = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    if browser.window_of(id) != Some(window) {
        return Err(HakuError::TabNotFound(id.0.to_string()));
    }
    Ok(browser.tab(id)?.slot())
}

/// Opens the print preview for the page in `slot`, reporting a failure in `window`.
fn print_slot(app: &tauri::AppHandle, window: WindowId, slot: SlotId) -> Result<()> {
    let Some(page) = app.get_webview(&slot.label()) else {
        return Ok(());
    };
    let report = app.clone();
    platform::print(&page, Box::new(move |error| report_failure(&report, window, error)))
}

/// Opens the Save As dialog for the page in `slot`, reporting a failure in `window`.
fn save_slot(app: &tauri::AppHandle, window: WindowId, slot: SlotId) -> Result<()> {
    let Some(page) = app.get_webview(&slot.label()) else {
        return Ok(());
    };
    let report = app.clone();
    platform::save_page(&page, Box::new(move |error| report_failure(&report, window, error)))
}

/// Finds `query` in a tab's page. [Find, print and save](../../../docs/specs/find-print-save.md).
///
/// A tab without a live page has nothing to find in, and is not an error.
#[tauri::command]
#[specta::specta]
pub async fn find_in_page(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
    query: String,
    direction: FindDirection,
) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    check_query(&query)?;
    match slot_of(&state, window, tab)?.and_then(|slot| app.get_webview(&slot.label())) {
        Some(page) => platform::find(&page, &query, direction),
        None => Ok(()),
    }
}

/// Ends a tab's find and clears its highlights. `refocus` gives the page the
/// keyboard back, for a bar the user closed; a bar closed because focus moved
/// elsewhere passes `false`.
#[tauri::command]
#[specta::specta]
pub async fn stop_find(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
    refocus: bool,
) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    // The bar closes as its tab does, so a tab already gone has no search.
    let slot = match slot_of(&state, window, tab) {
        Err(HakuError::TabNotFound(_)) => return Ok(()),
        found => found?,
    };
    let Some(page) = slot.and_then(|slot| app.get_webview(&slot.label())) else {
        return Ok(());
    };
    platform::stop_find(&page)?;
    if refocus {
        page.set_focus()?;
    }
    Ok(())
}

/// Opens the print preview for a tab's page. A failure once it is open
/// arrives as [`FailureReported`].
#[tauri::command]
#[specta::specta]
pub async fn print_page(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    match slot_of(&state, window, tab)? {
        Some(slot) => print_slot(&app, window, slot),
        None => Ok(()),
    }
}

/// Opens the engine's Save As dialog for a tab's page. A failure once it is
/// open arrives as [`FailureReported`].
#[tauri::command]
#[specta::specta]
pub async fn save_page(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: TabId,
) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    match slot_of(&state, window, tab)? {
        Some(slot) => save_slot(&app, window, slot),
        None => Ok(()),
    }
}

/// The extensions the extensions folder installed.
#[tauri::command]
#[specta::specta]
pub fn extensions(webview: tauri::Webview, state: State<'_, AppState>) -> Result<Extensions> {
    ensure_chrome(&webview)?;
    let extensions = state
        .extensions
        .read()
        .map_err(|_| HakuError::Storage("extensions lock poisoned".into()))?;
    Ok(extensions.clone())
}

/// Longest the engine is waited on for the window id of the tab a toolbar
/// popup acts on.
const ENGINE_WINDOW_TIMEOUT: Duration = Duration::from_millis(500);

/// Opens an extension's toolbar popup, for the named visible tab of the window that
/// called, just below `anchor`: the button it was opened from, in the
/// interface's logical pixels.
///
/// Opening it from the button of the popup open now closes it instead, as
/// in other browsers.
///
/// @returns Whether a popup opened: not when the click closed the one open.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] for an extension that is not
/// installed, has no popup or is switched off, or a hidden/foreign tab;
/// [`HakuError::TabNotFound`] for an unknown tab, and propagates the failure to
/// open the popup's window.
#[tauri::command]
#[specta::specta]
pub async fn open_extension_popup(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
    tab: TabId,
    anchor: webview::Viewport,
) -> Result<bool> {
    let window = ensure_chrome(&webview)?;
    let (slot, source) = {
        let browser = state
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        let slot = browser.extension_tab_in(window, tab)?.slot();
        let source = browser
            .state_in(window)
            .and_then(|snapshot| snapshot.viewports.into_iter().find(|pane| pane.tab == tab))
            .ok_or_else(|| HakuError::TabNotFound(tab.0.to_string()))?;
        (slot, source)
    };
    let url = state
        .extensions
        .read()
        .map_err(|_| HakuError::Storage("extensions lock poisoned".into()))?
        .installed
        .iter()
        .find(|extension| extension.id == id)
        .and_then(|extension| extension.popup.clone())
        .ok_or_else(|| HakuError::Unsupported(format!("{id} has no popup")))?;
    let disabled = state
        .settings
        .read()
        .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?
        .disabled_extensions
        .contains(&id);
    if disabled {
        return Err(HakuError::Unsupported(format!("{id} is switched off")));
    }
    {
        let mut popup = POPUP
            .lock()
            .map_err(|_| HakuError::Storage("popup lock poisoned".into()))?;
        let closed = popup.closed.take();
        if closed.is_some_and(|(closed, at)| closed == url && at.elapsed() < POPUP_TOGGLE) {
            return Ok(false);
        }
        popup.open = Some(url.clone());
    }

    // The tab the popup acts on, as the engine knows its window.
    let current = slot
        .and_then(|slot| app.get_webview(&slot.label()))
        .and_then(|page| platform::engine_window(&page, ENGINE_WINDOW_TIMEOUT));

    let native = app
        .get_window(&window.label())
        .ok_or_else(|| HakuError::WindowMissing(window.label()))?;
    let scale = native.scale_factor()?;
    let origin = native.inner_position()?.to_logical::<f64>(scale);
    let anchor = ScreenRect {
        x: origin.x + anchor.x,
        y: origin.y + anchor.y,
        width: anchor.width,
        height: anchor.height,
    };
    let area = native.current_monitor()?.map_or(anchor, |monitor| {
        let area = monitor.work_area();
        let position = area.position.to_logical::<f64>(scale);
        let size = area.size.to_logical::<f64>(scale);
        ScreenRect {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    });

    let label = webview::open_extension_popup(&app, window, &url, ExtensionView::Popup(current), popup_sink(&app))?;
    std::thread::spawn(move || {
        webview::fit_extension_popup(&app, &label, anchor, area, || {
            let is_current = app
                .state::<AppState>()
                .browser
                .read()
                .ok()
                .and_then(|browser| browser.state_in(window))
                .is_some_and(|snapshot| popup_source_is_current(&snapshot, &source));
            if !is_current {
                close_popup_when_free(&app);
            }
            is_current
        })
    });
    Ok(true)
}

/// Asks the user for an extension with the system's picker, owned by the
/// calling window, and adds it, or updates the installed one it is a version
/// of. Answers nothing when the picker was cancelled. The interface never
/// names a path: it only says what to pick.
#[tauri::command]
#[specta::specta]
pub async fn add_extension(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    source: ExtensionSource,
) -> Result<Option<AddedExtension>> {
    ensure_chrome(&webview)?;
    extensions::add(&app, &webview.window(), source)
}

/// Uninstalls the extension installed from `folder`, with what it stored, and
/// deletes the folder; or deletes a folder that failed to install.
#[tauri::command]
#[specta::specta]
pub async fn remove_extension(app: tauri::AppHandle, webview: tauri::Webview, folder: String) -> Result<()> {
    ensure_chrome(&webview)?;
    extensions::remove(&app, &folder)
}

/// Opens the extensions folder, creating it first, so an unpacked extension
/// can be dropped in.
#[tauri::command]
#[specta::specta]
pub fn open_extensions_folder(webview: tauri::Webview, state: State<'_, AppState>) -> Result<()> {
    ensure_chrome(&webview)?;
    std::fs::create_dir_all(&state.paths.extensions)
        .map_err(|error| HakuError::Storage(format!("{}: {error}", state.paths.extensions.display())))?;
    platform::reveal_folder(&state.paths.extensions)
}

/// The downloads this window lists, newest first: its own profile's only.
#[tauri::command]
#[specta::specta]
pub fn downloads(webview: tauri::Webview, state: State<'_, AppState>) -> Result<Vec<Download>> {
    let window = ensure_chrome(&webview)?;
    let private = is_private(&state, window)?;
    Ok(downloads::lock(&state).list(private))
}

#[tauri::command]
#[specta::specta]
pub async fn pause_download(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: DownloadId,
) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    control_download(&app, &state, window, id, DownloadAction::Pause)
}

/// Resumes a paused download, or retries a failed one the engine can
/// resume.
#[tauri::command]
#[specta::specta]
pub async fn resume_download(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: DownloadId,
) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    control_download(&app, &state, window, id, DownloadAction::Resume)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_download(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: DownloadId,
) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    control_download(&app, &state, window, id, DownloadAction::Cancel)
}

/// Opens a completed download's file with the program the system gives it.
///
/// Only the chrome reaches this, and only for a path Haku recorded: a page
/// can neither name a file nor ask for one to be opened.
#[tauri::command]
#[specta::specta]
pub async fn open_download(webview: tauri::Webview, state: State<'_, AppState>, id: DownloadId) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    let download = listed_download(&state, window, id)?;
    if download.state != DownloadState::Completed {
        return Err(HakuError::Unsupported(format!("download {} has not completed", id.0)));
    }
    let path = std::path::Path::new(&download.path);
    if !path.is_file() {
        return Err(HakuError::Storage(format!("{} is gone", path.display())));
    }
    platform::open_file(path)
}

/// Shows a download's file in its folder, or the folder when the file is
/// not there.
#[tauri::command]
#[specta::specta]
pub async fn show_download(webview: tauri::Webview, state: State<'_, AppState>, id: DownloadId) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    let download = listed_download(&state, window, id)?;
    let path = std::path::Path::new(&download.path);
    if path.is_file() {
        return platform::show_file(path);
    }
    match path.parent().filter(|folder| folder.is_dir()) {
        Some(folder) => platform::reveal_folder(folder),
        None => Err(HakuError::Storage(format!("{} is gone", path.display()))),
    }
}

/// Unlists this window's profile's ended downloads. Their files stay.
#[tauri::command]
#[specta::specta]
pub fn clear_downloads(app: tauri::AppHandle, webview: tauri::Webview, state: State<'_, AppState>) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    let private = is_private(&state, window)?;
    {
        // Held throughout, so a queued save cannot write back a download
        // between forgetting it and unlisting it. Forgotten first, so a failed
        // delete leaves the list as stored.
        let history = state
            .history
            .lock()
            .map_err(|_| HakuError::Storage("history lock poisoned".into()))?;
        let ended = downloads::lock(&state).ended(private);
        history.forget_downloads(&ended)?;
        downloads::lock(&state).unlist(&ended);
    }
    downloads::report(&app, &state);
    Ok(())
}

/// Closes a tab, or the window when it is nothing, that was held because it
/// would cancel downloads, cancelling them.
/// [Downloads § Closing](../../../docs/specs/downloads.md#closing).
#[tauri::command]
#[specta::specta]
pub async fn confirm_close(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    tab: Option<TabId>,
) -> Result<()> {
    let window = ensure_chrome(&webview)?;
    let ending = {
        let browser = state
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        if let Some(tab) = tab.filter(|tab| browser.window_of(*tab) != Some(window)) {
            return Err(HakuError::TabNotFound(tab.0.to_string()));
        }
        browser.ending(window, tab)
    };
    let cancelling = match ending {
        None => Vec::new(),
        Some(Ending::Quit) => downloads::lock(&state).active(None),
        Some(Ending::PrivateSession) => downloads::lock(&state).active(Some(Profile::Private)),
    };
    for id in &cancelling {
        let _ = platform::control_download(&app, *id, DownloadAction::Cancel);
    }
    let (cancelled, downloading) = {
        let mut downloads = downloads::lock(&state);
        (downloads.cancel_all(&cancelling), downloads.downloading())
    };
    // Written now: quitting does not wait for the download queue.
    if let Ok(history) = state.history.lock() {
        for download in &cancelled {
            let _ = history.save_download(download);
        }
    }
    downloads::report(&app, &state);
    match tab {
        Some(tab) => {
            let home = home_url(&state)?;
            // Held webviews are released even when the close fails, as the
            // browser has already recorded them.
            let mut closed = Ok(());
            mutate(&app, &state, |browser| {
                let mut effects = browser.set_downloading(downloading);
                match browser.close_tab(tab, &home) {
                    Ok(more) => effects.extend(more),
                    Err(error) => closed = Err(error),
                }
                Ok(effects)
            })?;
            closed
        }
        None => {
            mutate(&app, &state, |browser| Ok(browser.set_downloading(downloading)))?;
            close_window(&app, window);
            Ok(())
        }
    }
}

fn is_private(state: &AppState, window: WindowId) -> Result<bool> {
    Ok(state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?
        .is_private(window))
}

/// A download as the window asking about it lists it.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] for one the window does not list, which
/// includes every download of the other profile.
fn listed_download(state: &AppState, window: WindowId, id: DownloadId) -> Result<Download> {
    let private = is_private(state, window)?;
    downloads::lock(state)
        .entry(id)
        .filter(|download| download.private == private)
        .cloned()
        .ok_or_else(|| HakuError::Unsupported(format!("download {} is not listed in this window", id.0)))
}

fn control_download(
    app: &tauri::AppHandle,
    state: &AppState,
    window: WindowId,
    id: DownloadId,
    action: DownloadAction,
) -> Result<()> {
    listed_download(state, window, id)?;
    if !platform::control_download(app, id, action)? {
        // The engine let go of it with the webview it ran in.
        let mut downloads = downloads::lock(state);
        if action == DownloadAction::Cancel {
            downloads.cancel_all(&[id]);
        }
        downloads.lose(id);
    }
    downloads::queue(app, vec![id]);
    Ok(())
}

// A popup can still be hidden when its captured page is replaced. Native
// focus loss alone cannot close it then; check before showing and while fitting.
fn popup_source_is_current(snapshot: &BrowserState, source: &crate::browser::ViewportAssignment) -> bool {
    snapshot.focused_viewport == source.id
        && snapshot
            .viewports
            .iter()
            .any(|pane| pane.id == source.id && pane.tab == source.tab)
}

#[cfg(test)]
mod popup_tests {
    use super::*;

    #[test]
    fn toolbar_popup_closes_on_pane_focus_change_or_source_reassignment() {
        let mut browser = Browser::new(2);
        let (left, _) = browser.open_tab("haku://settings", true);
        let (right, _) = browser.open_tab("haku://history", false);
        let window = browser.main_window();
        browser.split_tab(window, right).unwrap();
        let snapshot = browser.state_in(window).unwrap();
        let source = snapshot.viewports[1].clone();
        assert!(popup_source_is_current(&snapshot, &source));
        browser.focus_pane(window, left).unwrap();
        assert!(!popup_source_is_current(&browser.state_in(window).unwrap(), &source));
        browser.focus_pane(window, right).unwrap();
        assert!(popup_source_is_current(&browser.state_in(window).unwrap(), &source));
        browser.open_tab("haku://memory", true);
        assert!(!popup_source_is_current(&browser.state_in(window).unwrap(), &source));
    }
}
