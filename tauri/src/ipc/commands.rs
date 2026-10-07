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

use std::sync::Arc;
use std::time::Duration;

use tauri::{Manager, State};
use tauri_specta::Event;

use crate::browser::{resolve_target, BrowserState, Direction, Effect, PageReport};
use crate::chrome::{self, Layout};
use crate::error::{HakuError, Result};
use crate::model::{DialogAnswer, DialogId, Preset, Pressure, SlotId, TabId};
use crate::platform::{self, PageSignal};
use crate::state::{now_ms, AppState, MemoryReport};
use crate::storage::history_db::HistoryEntry;
use crate::storage::Settings;
use crate::webview::{self, PageObserver, CHROME_LABEL};

use super::events::{MemoryChanged, SettingsChanged, StateChanged};

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
            PageSignal::AudioChanged { playing } => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let _ = mutate(&app, &state, |browser| Ok(browser.report_audio(slot, playing)));
                });
            }
        },
    )
}

/// How often the tick runs while memory is plentiful.
const TICK: Duration = Duration::from_secs(5);

/// How often the tick runs under pressure, when memory runs out faster than
/// a slower timer would notice.
const TICK_PRESSED: Duration = Duration::from_secs(1);

/// Measures memory and runs smart discarding for as long as the application
/// does.
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

            let effects = match state.browser.write() {
                Ok(mut browser) => browser.relieve(now_ms(), platform::memory_is_low()),
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

fn report_memory(state: &AppState) -> Result<MemoryReport> {
    let memory = state
        .memory
        .read()
        .map_err(|_| HakuError::Storage("memory lock poisoned".into()))?;
    let browser = state
        .browser
        .read()
        .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    Ok(memory.report(browser.slots()))
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
    webview::apply(app, effects, state.viewport(), &page_observer())?;

    let snapshot = {
        let browser = state
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        browser.state()
    };

    StateChanged(snapshot.clone()).emit(app).map_err(HakuError::from)?;
    let _ = state.save_session();
    Ok(snapshot)
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
/// Resolved here rather than in the interface because the slot count depends
/// on the machine's memory, which only Rust can read.
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
        current.clone().with_preset(preset, platform::total_memory())
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
    Ok(settings.preset(platform::total_memory()))
}

fn store_settings(app: &tauri::AppHandle, state: &AppState, settings: Settings) -> Result<Settings> {
    let settings = settings.sanitized();

    {
        let mut current = state
            .settings
            .write()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        *current = settings.clone();
    }
    state.save_settings()?;

    let (capacity, freeze, discard) = (settings.pool_capacity(), settings.freeze_tabs, settings.discard_tabs);
    mutate(app, state, |browser| {
        Ok(browser.set_optimization(capacity, freeze, discard))
    })?;

    SettingsChanged(settings.clone()).emit(app).map_err(HakuError::from)?;
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

    mutate(&app, &state, |browser| {
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
    mutate(&app, &state, |browser| browser.select_tab(id, now_ms()))
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
