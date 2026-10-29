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

use crate::browser::{resolve_target, Browser, BrowserState, Direction, Effect, Opening, PageReport, Pick};
use crate::chrome::{self, Layout};
use crate::error::{HakuError, Result};
use crate::model::{
    data_url, icon_file, is_internal, jpeg_data_url, popup_url, ContextMenuRequestId, DialogAnswer, DialogId,
    Extension, Extensions, PageMenu, Placement, Preset, Pressure, ScreenRect, Shortcut, SlotId, TabId, WindowId,
    WindowKind, WindowRequestId,
};
use crate::platform::{self, KeySink, PageSignal, ProcessFailure};
use crate::state::{now_ms, AppState, MemoryReport};
use crate::storage::history_db::HistoryEntry;
use crate::storage::{self, InstalledExtension, InstalledExtensions, Settings, WindowBounds};
use crate::webview::inject::ExtensionView;
use crate::webview::{self, Departure, Hooks, PageObserver, PageReading};

use super::events::{
    AddressFocusRequested, ContextMenuRequested, ExtensionsChanged, MemoryChanged, SettingsChanged, StateChanged,
};
use super::recovery;

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
            // Recorded before the page commits, on this thread, so the
            // navigation it belongs to finds it.
            PageSignal::NavigationStarted { form } => {
                if let Ok(mut browser) = app.state::<AppState>().browser.write() {
                    browser.report_navigation(slot, form);
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
                let window = app.state::<AppState>().browser.read().ok().map(|browser| {
                    browser
                        .tab_in(slot)
                        .and_then(|tab| browser.window_of(tab))
                        .unwrap_or_else(|| browser.main_window())
                });
                if let Some(window) = window {
                    queue_shortcut(app, window, shortcut);
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
fn queue_window(app: &tauri::AppHandle, window: WindowRequest) {
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
        slot,
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
    let _ = mutate(app, &state, |browser| {
        browser.set_pressure(pressure);
        Ok(match slot {
            Some(slot) => browser.open_from(slot, url, opening),
            None => browser.open_requested(url, opening),
        }
        .1)
    });
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
pub fn shortcut_sink(app: tauri::AppHandle, window: WindowId) -> KeySink {
    Arc::new(move |shortcut| queue_shortcut(&app, window, shortcut))
}

/// Runs shortcuts one at a time, in the order they were pressed.
///
/// A shortcut changes the browser and then applies the change to webviews,
/// and only the first half holds the browser's lock. Run side by side, two
/// quick Ctrl+Tabs could show their pages in the opposite order to the one
/// the browser recorded, leaving the selected tab showing another's page.
fn queue_shortcut(app: &tauri::AppHandle, window: WindowId, shortcut: Shortcut) {
    static QUEUE: OnceLock<mpsc::Sender<(WindowId, Shortcut)>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        let app = app.clone();
        std::thread::spawn(move || {
            for (window, shortcut) in receiver {
                run_shortcut(&app, window, shortcut);
            }
        });
        sender
    });
    let _ = queue.send((window, shortcut));
}

/// Does what a shortcut asks, to the active tab of the window it was pressed
/// in.
///
/// Off the UI thread, like a command: most shortcuts drive webviews. A failure
/// has nowhere to be shown, since no command call is waiting on it, and leaves
/// the browser as it was.
fn run_shortcut(app: &tauri::AppHandle, window: WindowId, shortcut: Shortcut) {
    let state = app.state::<AppState>();
    let Some(active) = state.browser.read().ok().and_then(|browser| browser.active_in(window)) else {
        return;
    };
    let select = |pick: Pick| {
        let pressure = read_pressure(&state);
        mutate(app, &state, |browser| {
            browser.set_pressure(pressure);
            match browser.pick_in(window, pick) {
                Some(id) => browser.select_tab(id, now_ms()),
                None => Ok(Vec::new()),
            }
        })
    };
    let change = |change: &dyn Fn(&mut Browser) -> Result<Vec<Effect>>| mutate(app, &state, |browser| change(browser));

    let _ = match shortcut {
        Shortcut::NewTab => open_home(app, &state, |browser, home| browser.open_tab_in(window, home, true)),
        Shortcut::NewWindow => open_home(app, &state, Browser::open_window),
        Shortcut::CloseTab => home_url(&state).and_then(|home| change(&|browser| browser.close_tab(active, &home))),
        Shortcut::ReopenClosedTab => change(&|browser| Ok(browser.reopen_closed_tab())),
        Shortcut::NextTab => select(Pick::Next),
        Shortcut::PreviousTab => select(Pick::Previous),
        Shortcut::NthTab(index) => select(Pick::Nth(index)),
        Shortcut::LastTab => select(Pick::Last),
        Shortcut::FocusAddress => focus_address(app, active),
        Shortcut::Reload => change(&|browser| browser.reload(active)),
        Shortcut::Back => change(&|browser| browser.go_back(active)),
        Shortcut::Forward => change(&|browser| browser.go_forward(active)),
        Shortcut::DevTools => {
            let slot = state
                .browser
                .read()
                .ok()
                .and_then(|browser| browser.tab(active).ok()?.slot());
            if let Some(target) = slot.and_then(|slot| app.get_webview(&slot.label())) {
                target.open_devtools();
            }
            Ok(())
        }
    };
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
                let _ = open_home(&app, &state, Browser::open_window);
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

/// Installs every extension in the extensions folder into the shared profile,
/// switches each on or off as the settings say, and uninstalls the ones Haku
/// installed before whose folder is gone.
///
/// Only ids Haku recorded installing are uninstalled: the engine lists its own
/// built-in extensions alongside the user's, with nothing to tell them apart,
/// and refuses to remove those. One that fails to uninstall stays recorded, to
/// be tried again at the next start.
///
/// Runs once, at startup and off the UI thread: the engine has to be asked
/// and waited on for each extension.
///
/// A failure is kept in [`Extensions::error`] for the settings page to show,
/// since nothing is waiting on startup to report it.
pub fn sync_extensions(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let extensions = install_extensions(app, &state).unwrap_or_else(|error| Extensions {
        error: Some(error),
        ..Extensions::default()
    });
    if let Ok(mut stored) = state.extensions.write() {
        *stored = extensions.clone();
    }
    webview::extensions_installed(app);
    let _ = ExtensionsChanged(extensions).emit(app);
}

/// # Errors
/// Fails when the chrome webview cannot be reached. A folder that does not
/// install is reported in [`Extensions::failed`], and any other step that
/// fails in [`Extensions::error`], without stopping the rest.
fn install_extensions(app: &tauri::AppHandle, state: &AppState) -> Result<Extensions> {
    let chrome = webview::chrome(app)?;
    let mut problems = Vec::new();

    let mut installed = Vec::new();
    let mut now = Vec::new();
    let mut failed = Vec::new();
    let mut folders: Vec<std::path::PathBuf> = match std::fs::read_dir(&state.paths.extensions) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect(),
        // No folder yet means no extensions.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            problems.push(HakuError::from(error));
            Vec::new()
        }
    };
    // The order the file system lists folders in is not stable.
    folders.sort();
    for folder in folders {
        match platform::install_extension(&chrome, &folder) {
            Ok((id, name)) => {
                let manifest = std::fs::read_to_string(folder.join("manifest.json")).unwrap_or_default();
                let icon = icon_file(&manifest).and_then(|(path, mime)| {
                    std::fs::read(folder.join(path))
                        .ok()
                        .map(|bytes| data_url(mime, &bytes))
                });
                now.push(InstalledExtension {
                    id: id.clone(),
                    folder: folder.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                });
                installed.push(Extension {
                    popup: popup_url(&id, &manifest),
                    icon,
                    id,
                    name,
                });
            }
            Err(_) => failed.push(folder.file_name().unwrap_or_default().to_string_lossy().into_owned()),
        }
    }

    let disabled = state
        .settings
        .read()
        .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?
        .disabled_extensions
        .clone();
    for extension in &installed {
        let enabled = !disabled.contains(&extension.id);
        if let Err(error) = platform::set_extension_enabled(&chrome, extension.id.clone(), enabled) {
            problems.push(error);
        }
    }

    let before: InstalledExtensions = storage::read_json(&state.paths.installed_extensions);
    let mut recorded: Vec<InstalledExtension> = before
        .installed
        .iter()
        .filter(|old| !now.iter().any(|new| new.id == old.id))
        .cloned()
        .collect();
    // An extension with a `key` keeps its id when its folder moves: still
    // installed, from its new folder.
    let gone = before
        .removed(|folder| state.paths.extensions.join(folder).is_dir())
        .into_iter()
        .filter(|old| !now.iter().any(|new| new.id == old.id));
    for gone in gone {
        match platform::uninstall_extension(&chrome, gone.id.clone()) {
            Ok(()) => recorded.retain(|kept| kept.id != gone.id),
            Err(error) => problems.push(error),
        }
    }
    recorded.extend(now);
    if let Err(error) = storage::write_json(
        &state.paths.installed_extensions,
        &InstalledExtensions { installed: recorded },
    ) {
        problems.push(error);
    }

    Ok(Extensions {
        installed,
        failed,
        error: problems.into_iter().next(),
    })
}

/// Switches installed extensions on or off where the settings changed them.
fn switch_extensions(app: &tauri::AppHandle, state: &AppState, before: &[String], after: &[String]) -> Result<()> {
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
                    let mut effects: Vec<Effect> = readings
                        .into_iter()
                        .flat_map(|(tab, reading)| browser.report_state(tab, reading))
                        .collect();
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

    let recorded = {
        let Ok(mut browser) = state.browser.write() else {
            return;
        };
        browser.report_page(slot, commits, title)
    };

    let Some(report) = recorded else {
        return;
    };
    queue_record(app, report);

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

fn slot_viewports(state: &AppState) -> Result<HashMap<SlotId, crate::browser::ViewportTarget>> {
    Ok(state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?
        .slot_viewports())
}

/// Applies effects to real webviews and tells every window what changed.
///
/// What the browser decided is shown and saved even when an effect failed,
/// since the browser already holds it; the first failure is returned after.
fn commit(app: &tauri::AppHandle, state: &AppState, effects: &[Effect]) -> Result<()> {
    let hooks = hooks();
    let placements = slot_viewports(state)?;
    let (departures, mut applied) = webview::apply(app, effects, &state.viewports, &hooks, &placements);
    let mut readings = keep_previews(state, departures);
    // A page read as it was left may turn out to be capturing, which changes
    // what should happen to it. Bounded, though a reading only ever leads to
    // freezing or resuming one tab.
    for _ in 0..MAX_READING_ROUNDS {
        if readings.is_empty() {
            break;
        }
        let effects = report_readings(state, readings)?;
        let placements = slot_viewports(state)?;
        let (departures, more) = webview::apply(app, &effects, &state.viewports, &hooks, &placements);
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
fn mutate<F>(app: &tauri::AppHandle, state: &AppState, mutate: F) -> Result<()>
where
    F: FnOnce(&mut Browser) -> Result<Vec<Effect>>,
{
    state
        .changes
        .run(&state.browser, mutate, |effects| commit(app, state, effects))
}

/// [`mutate`], for a command a window's interface called.
///
/// @returns What that window shows now, or what the normal window used last
///   shows when the change closed it.
fn mutate_in<F>(app: &tauri::AppHandle, state: &AppState, window: WindowId, change: F) -> Result<BrowserState>
where
    F: FnOnce(&mut Browser) -> Result<Vec<Effect>>,
{
    mutate(app, state, change)?;
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
        _ => {}
    }
}

/// Closes a browser window, or quits when it is the last one.
///
/// Off the UI thread: closing drives webviews.
fn close_window(app: &tauri::AppHandle, window: WindowId) {
    let state = app.state::<AppState>();
    let last = state
        .browser
        .read()
        .map_or(true, |browser| browser.is_last_window(window));
    if last {
        // The session already holds the window's tabs; only where it was may
        // have changed since it was last saved.
        let _ = state.save_session();
        app.exit(0);
        return;
    }
    let first = webview::chrome(app).ok().map(|chrome| chrome.label().to_string());
    let _ = mutate(app, &state, |browser| Ok(browser.close_window(window)));
    if let Ok(mut bounds) = state.bounds.write() {
        bounds.remove(&window);
    }
    // The service worker watch is attached to the first window's chrome,
    // and goes with it.
    if first.as_deref() == Some(window.label().as_str()) {
        if let Ok(chrome) = webview::chrome(app) {
            let _ = platform::stop_idle_workers(&chrome);
        }
    }
}

/// Remembers where a window is on screen, for the session. A minimised
/// window is remembered where it was before, and a maximised one keeps the
/// rectangle it returns to. A window maximised before it ever had another
/// rectangle is not remembered, so it opens maximized again.
fn record_bounds(window: &tauri::Window, id: WindowId, state: &AppState) {
    if window.is_minimized().unwrap_or(false) {
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
    let browser = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    browser
        .state_in(window)
        .ok_or_else(|| HakuError::WindowMissing(window.label()))
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
    ensure_chrome(&webview)?;
    store_settings(&app, &state, settings)
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
    ensure_chrome(&webview)?;
    let settings = {
        let current = state
            .settings
            .read()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        current.clone().with_preset(preset)
    };
    store_settings(&app, &state, settings)
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

fn store_settings(app: &tauri::AppHandle, state: &AppState, settings: Settings) -> Result<Settings> {
    let settings = settings.sanitized();

    // One store at a time, from replacing through applying, so a failed save
    // never restores settings older than a store that overlapped, and the
    // browser runs with the settings stored last (issue #41). Held across the
    // main thread driving webviews; that is safe because no main-thread path
    // takes it and both callers are async.
    static STORING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let storing = STORING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous = {
        let mut current = state
            .settings
            .write()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        std::mem::replace(&mut *current, settings.clone())
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
    let announced = SettingsChanged(settings.clone()).emit(app);

    let (capacity, freeze, discard) = (settings.pool_capacity(), settings.freeze_tabs, settings.discard_tabs);
    let (budget, kept_sites) = (settings.kept_memory_bytes(), settings.kept_sites.clone());
    mutate(app, state, |browser| {
        browser.set_keeping(budget, kept_sites);
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
        Ok(browser.open_window(target).1)
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
    state.changes.run(
        &state.browser,
        |browser| {
            accepted.set(state.set_layout(browser, window, &layout)?);
            Ok(if accepted.get() {
                browser.reconcile()
            } else {
                Vec::new()
            })
        },
        |effects| {
            if accepted.get() {
                let scale = webview.window().scale_factor()?;
                // Bounds, mask and reconciliation share the serialized change turn.
                let bounds = {
                    let browser = state
                        .browser
                        .read()
                        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
                    state.viewports.slot_bounds(&browser, window)
                };
                for (slot, rect) in bounds {
                    webview::set_bounds(&app, slot, rect)?;
                }
                chrome::apply_layout(&webview, &layout, scale)?;
                commit(&app, &state, effects)?;
            }
            Ok(())
        },
    )?;
    let browser = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    Ok(browser.state_in(window).unwrap_or_else(|| browser.state()))
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

/// Opens an extension's toolbar popup, for the active tab of the window that
/// called, just below `anchor`: the button it was opened from, in the
/// interface's logical pixels.
///
/// Opening it from the button of the popup open now closes it instead, as
/// in other browsers.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] for an extension that is not
/// installed, has no popup or is switched off, and propagates the failure to
/// open the popup's window.
#[tauri::command]
#[specta::specta]
pub async fn open_extension_popup(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
    anchor: webview::Viewport,
) -> Result<()> {
    let window = ensure_chrome(&webview)?;
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
            return Ok(());
        }
        popup.open = Some(url.clone());
    }

    // The tab the popup acts on, as the engine knows its window.
    let slot = {
        let browser = state
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        browser.active_in(window).and_then(|tab| browser.tab(tab).ok()?.slot())
    };
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
    std::thread::spawn(move || webview::fit_extension_popup(&app, &label, anchor, area));
    Ok(())
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
