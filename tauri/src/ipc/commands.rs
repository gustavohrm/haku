//! The command surface the interface drives Haku through.
//!
//! ## Trust
//!
//! Application commands are reachable from any webview in the process, and
//! content webviews load arbitrary remote pages. Every command here therefore
//! rejects callers that are not the chrome.
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

use std::sync::{mpsc, Arc, OnceLock};
use std::time::Duration;

use tauri::{Manager, State};
use tauri_specta::Event;

use crate::browser::{resolve_target, BrowserState, Direction, Effect, Opening, PageReport, Pick};
use crate::chrome::{self, Layout};
use crate::error::{HakuError, Result};
use crate::model::{
    is_internal, jpeg_data_url, popup_url, DialogAnswer, DialogId, Extension, Extensions, Preset, Pressure, Shortcut,
    SlotId, TabId, WindowRequestId,
};
use crate::platform::{self, KeySink, PageSignal};
use crate::state::{now_ms, AppState, MemoryReport};
use crate::storage::history_db::HistoryEntry;
use crate::storage::Settings;
use crate::webview::{self, Departure, PageObserver, PageReading, CHROME_LABEL};

use super::events::{AddressFocusRequested, ExtensionsChanged, MemoryChanged, SettingsChanged, StateChanged};

/// Rejects a command that only the interface may issue.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the caller is a content webview.
fn ensure_chrome(webview: &tauri::Webview) -> Result<()> {
    if webview.label() == CHROME_LABEL {
        return Ok(());
    }
    Err(HakuError::Unsupported(format!(
        "{} may not call browser commands",
        webview.label()
    )))
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
                    .and_then(|mut browser| browser.report_loaded(slot, &url).then(|| browser.state()));
                if let Some(snapshot) = loaded {
                    let _ = StateChanged(snapshot).emit(app);
                }
            }
            PageSignal::Shortcut(shortcut) => queue_shortcut(app, shortcut),
            PageSignal::WindowRequested {
                request,
                url,
                popup,
                background,
                gesture,
            } => {
                let app = app.clone();
                std::thread::spawn(move || {
                    if gesture {
                        open_window(&app, slot, request, url, popup, background);
                    }
                    let _ = platform::refuse_window(&app, request);
                });
            }
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

/// Opens what the page in `slot` asked to open in a new window in a tab
/// instead: Haku opens no windows yet.
///
/// A popup's tab is handed a new webview so the page can still reach it, and
/// so is anything that is not an ordinary web address, such as a blank page
/// the page writes into or a `blob:` URL, which only the engine can load on
/// the page's behalf. A link to a web address opens as an ordinary tab. A page
/// never opens one of Haku's own pages. The request is answered by the
/// caller, if handing it a webview did not answer it.
///
/// As in other browsers, a page opens windows only in answer to the user:
/// a request the page made on its own never reaches here.
fn open_window(
    app: &tauri::AppHandle,
    slot: SlotId,
    request: WindowRequestId,
    url: String,
    popup: bool,
    background: bool,
) {
    if is_internal(&url) {
        return;
    }
    let web = url.starts_with("https://") || url.starts_with("http://");
    let opening = match (popup || !web, background) {
        (true, _) => Opening::Connected(request),
        (false, true) => Opening::Background,
        (false, false) => Opening::Foreground,
    };
    let state = app.state::<AppState>();
    let pressure = read_pressure(&state);
    let _ = mutate(app, &state, |browser| {
        browser.set_pressure(pressure);
        Ok(browser.open_from(slot, url, opening).1)
    });
}

/// Runs the shortcuts pressed while the interface has focus. Pages report
/// theirs through [`page_observer`], so both reach [`run_shortcut`].
pub fn shortcut_sink(app: tauri::AppHandle) -> KeySink {
    Arc::new(move |shortcut| queue_shortcut(&app, shortcut))
}

/// Runs shortcuts one at a time, in the order they were pressed.
///
/// A shortcut changes the browser and then applies the change to webviews,
/// and only the first half holds the browser's lock. Run side by side, two
/// quick Ctrl+Tabs could show their pages in the opposite order to the one
/// the browser recorded, leaving the selected tab showing another's page.
fn queue_shortcut(app: &tauri::AppHandle, shortcut: Shortcut) {
    static QUEUE: OnceLock<mpsc::Sender<Shortcut>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        let app = app.clone();
        std::thread::spawn(move || {
            for shortcut in receiver {
                run_shortcut(&app, shortcut);
            }
        });
        sender
    });
    let _ = queue.send(shortcut);
}

/// Does what a shortcut asks, to the active tab.
///
/// Off the UI thread, like a command: most shortcuts drive webviews. A failure
/// has nowhere to be shown, since no command call is waiting on it, and leaves
/// the browser as it was.
fn run_shortcut(app: &tauri::AppHandle, shortcut: Shortcut) {
    let state = app.state::<AppState>();
    let Some(active) = state.browser.read().ok().and_then(|browser| browser.active()) else {
        return;
    };
    let select = |pick: Pick| {
        let pressure = read_pressure(&state);
        mutate(app, &state, |browser| {
            browser.set_pressure(pressure);
            match browser.pick(pick) {
                Some(id) => browser.select_tab(id, now_ms()),
                None => Ok(Vec::new()),
            }
        })
        .map(drop)
    };
    let change = |change: &dyn Fn(&mut crate::browser::Browser) -> Result<Vec<Effect>>| {
        mutate(app, &state, |browser| change(browser)).map(drop)
    };

    let _ = match shortcut {
        Shortcut::NewTab => home_url(&state).and_then(|home| {
            let pressure = read_pressure(&state);
            let mut opened = None;
            mutate(app, &state, |browser| {
                browser.set_pressure(pressure);
                let (id, effects) = browser.open_tab(home, true);
                opened = Some(id);
                Ok(effects)
            })?;
            // As in every browser, a new tab starts in the address field.
            opened.map_or(Ok(()), |id| focus_address(app, id))
        }),
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

/// Moves keyboard focus to the interface and asks it to focus the address
/// field for `tab`, which may not have rendered yet.
fn focus_address(app: &tauri::AppHandle, tab: TabId) -> Result<()> {
    webview::chrome(app)?.set_focus()?;
    AddressFocusRequested(tab).emit(app).map_err(HakuError::from)
}

fn home_url(state: &AppState) -> Result<String> {
    let settings = state
        .settings
        .read()
        .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
    Ok(settings.home_url.clone())
}

/// Installs every extension in the extensions folder into the shared profile
/// and switches each on or off as the settings say.
///
/// An extension whose folder was deleted is not uninstalled: the engine lists
/// its own built-in extensions alongside the user's, with nothing to tell them
/// apart, and refuses to remove those.
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
                installed.push(Extension {
                    popup: popup_url(&id, &manifest),
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
            let effects = match state.browser.write() {
                Ok(mut browser) => {
                    let mut effects: Vec<Effect> = readings
                        .into_iter()
                        .flat_map(|(tab, reading)| browser.report_state(tab, reading))
                        .collect();
                    effects.extend(browser.tick(now_ms(), pressure, memory));
                    effects
                }
                Err(_) => continue,
            };
            // Most passes only re-time the visible tab, which is not worth an
            // event or a session write.
            if !effects.is_empty() {
                let _ = commit(&app, &state, &effects);
            }
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
    record_visit(state, &report);
    // The session should reopen where the page actually is, not where it was
    // opened.
    let _ = state.save_session();

    if let Ok(browser) = state.browser.read() {
        let _ = StateChanged(browser.state()).emit(app);
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

/// Applies effects to real webviews and tells the interface what changed.
fn commit(app: &tauri::AppHandle, state: &AppState, effects: &[Effect]) -> Result<BrowserState> {
    let mut readings = keep_previews(state, webview::apply(app, effects, state.viewport(), &page_observer())?);
    // A page read as it was left may turn out to be capturing, which changes
    // what should happen to it. Bounded, though a reading only ever leads to
    // freezing or resuming one tab.
    for _ in 0..MAX_READING_ROUNDS {
        if readings.is_empty() {
            break;
        }
        let effects = report_readings(state, readings)?;
        readings = keep_previews(
            state,
            webview::apply(app, &effects, state.viewport(), &page_observer())?,
        );
    }

    let snapshot = {
        let browser = state
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        browser.state()
    };
    // A closed tab's capture shows a page nobody can return to.
    if let Ok(mut previews) = state.previews.lock() {
        previews.retain(|id| snapshot.tabs.iter().any(|tab| tab.id == id));
    }

    StateChanged(snapshot.clone()).emit(app).map_err(HakuError::from)?;
    let _ = state.save_session();
    Ok(snapshot)
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
/// blocks cannot stall the next command.
fn mutate<F>(app: &tauri::AppHandle, state: &AppState, mutate: F) -> Result<BrowserState>
where
    F: FnOnce(&mut crate::browser::Browser) -> Result<Vec<Effect>>,
{
    let effects = {
        let mut browser = state
            .browser
            .write()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        mutate(&mut browser)?
    };
    commit(app, state, &effects)
}

#[tauri::command]
#[specta::specta]
pub fn get_state(webview: tauri::Webview, state: State<'_, AppState>) -> Result<BrowserState> {
    ensure_chrome(&webview)?;
    let browser = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    Ok(browser.state())
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

    let previous = {
        let mut current = state
            .settings
            .write()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        std::mem::replace(&mut *current, settings.clone())
    };
    state.save_settings()?;

    let (capacity, freeze, discard) = (settings.pool_capacity(), settings.freeze_tabs, settings.discard_tabs);
    let (budget, kept_sites) = (settings.kept_memory_bytes(), settings.kept_sites.clone());
    mutate(app, state, |browser| {
        browser.set_keeping(budget, kept_sites);
        Ok(browser.set_optimization(capacity, freeze, discard))
    })?;

    SettingsChanged(settings.clone()).emit(app).map_err(HakuError::from)?;
    // Last, so a switch the engine refuses is reported without leaving the
    // interface showing other settings than the ones stored. The stored
    // setting stands and is applied again at the next start.
    switch_extensions(app, state, &previous.disabled_extensions, &settings.disabled_extensions)?;
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
    ensure_chrome(&webview)?;
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
    mutate(&app, &state, |browser| {
        browser.set_pressure(pressure);
        let (_, effects) = browser.open_tab(target, activate);
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
    ensure_chrome(&webview)?;
    let home = {
        let settings = state
            .settings
            .read()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        settings.home_url.clone()
    };
    mutate(&app, &state, |browser| browser.close_tab(id, &home))
}

#[tauri::command]
#[specta::specta]
pub async fn select_tab(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
) -> Result<BrowserState> {
    ensure_chrome(&webview)?;
    let pressure = read_pressure(&state);
    mutate(&app, &state, |browser| {
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
    ensure_chrome(&webview)?;
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
    mutate(&app, &state, |browser| browser.navigate(id, target))
}

#[tauri::command]
#[specta::specta]
pub async fn go_back(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
) -> Result<BrowserState> {
    ensure_chrome(&webview)?;
    mutate(&app, &state, |browser| browser.go_back(id))
}

#[tauri::command]
#[specta::specta]
pub async fn go_forward(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
) -> Result<BrowserState> {
    ensure_chrome(&webview)?;
    mutate(&app, &state, |browser| browser.go_forward(id))
}

#[tauri::command]
#[specta::specta]
pub async fn reload_tab(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: TabId,
) -> Result<BrowserState> {
    ensure_chrome(&webview)?;
    mutate(&app, &state, |browser| browser.reload(id))
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
    ensure_chrome(&webview)?;
    mutate(&app, &state, |browser| browser.set_fixed(id, fixed))
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
    ensure_chrome(&webview)?;
    mutate(&app, &state, |browser| {
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
    ensure_chrome(&webview)?;

    state.set_layout(layout.clone());

    let slots = {
        let browser = state
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        browser.slot_ids()
    };

    if let Some(viewport) = layout.viewport {
        webview::set_bounds(&app, &slots, viewport)?;
    }

    let window = app
        .get_window(webview::MAIN_WINDOW_LABEL)
        .ok_or_else(|| HakuError::WindowMissing(webview::MAIN_WINDOW_LABEL.into()))?;
    let scale = window.scale_factor()?;
    chrome::apply_layout(&webview::chrome(&app)?, &layout, scale)?;

    // A restored session has tabs but no webviews, because until now there was
    // nowhere on screen to put one. Reconciling here is what loads the page the
    // window opens on.
    mutate(&app, &state, |browser| Ok(browser.reconcile()))
}

/// Records a visit, or retitles the last one, tolerating a failure rather than
/// breaking navigation.
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
    ensure_chrome(&webview)?;
    mutate(&app, &state, |browser| browser.answer_dialog(tab, dialog, answer))
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
