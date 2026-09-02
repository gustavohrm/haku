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

use tauri::{Manager, State};
use tauri_specta::Event;

use crate::browser::{resolve_target, BrowserState, Effect};
use crate::chrome::{self, Layout};
use crate::error::{HakuError, Result};
use crate::model::{SlotId, TabId};
use crate::state::{now_ms, AppState};
use crate::storage::history_db::HistoryEntry;
use crate::storage::Settings;
use crate::webview::{self, PageObserver, PageUpdate, CHROME_LABEL};

use super::events::{SettingsChanged, StateChanged};

/// Rejects a command that only the interface may issue.
///
/// # Errors
/// Returns [`HakuError::Unsupported`] when the caller is a content webview.
fn ensure_chrome(webview: &tauri::Webview) -> Result<()> {
    if webview.label() == CHROME_LABEL {
        return Ok(());
    }
    Err(HakuError::Unsupported(format!("{} may not call browser commands", webview.label())))
}

/// Observes what content webviews load, without the pages taking part.
///
/// Tauri withholds its IPC bridge from remote origins unless a capability opts
/// them in, and Haku deliberately does not. Page metadata therefore arrives
/// through Rust-side webview hooks, which is both safer and one less moving
/// part than a script reporting on a page's behalf.
fn page_observer() -> PageObserver<tauri::Wry> {
    Arc::new(|app: &tauri::AppHandle, slot: SlotId, update: PageUpdate| {
        let Some(handle) = app.try_state::<AppState>() else {
            return;
        };
        // `inner` re-borrows from the app rather than the local handle, so the
        // lock guards below can outlive it.
        let state: &AppState = handle.inner();

        let recorded = {
            let Ok(mut browser) = state.browser.write() else {
                return;
            };
            browser.report_page(slot, update.url, update.title)
        };

        let Some((url, title)) = recorded else {
            return;
        };
        record_visit(state, &url, &title);
        // The page just told us its real title; the stored session should carry
        // it rather than the URL it was opened with.
        let _ = state.save_session();

        if let Ok(browser) = state.browser.read() {
            let _ = StateChanged(browser.state()).emit(app);
        }
    })
}

/// Applies effects to real webviews and tells the interface what changed.
fn commit(app: &tauri::AppHandle, state: &AppState, effects: &[Effect]) -> Result<BrowserState> {
    webview::apply(app, effects, state.viewport(), &page_observer())?;

    let snapshot = {
        let browser = state.browser.read().map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
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
        let mut browser = state.browser.write().map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        mutate(&mut browser)?
    };
    commit(app, state, &effects)
}

#[tauri::command]
#[specta::specta]
pub fn get_state(webview: tauri::Webview, state: State<'_, AppState>) -> Result<BrowserState> {
    ensure_chrome(&webview)?;
    let browser = state.browser.read().map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
    Ok(browser.state())
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(webview: tauri::Webview, state: State<'_, AppState>) -> Result<Settings> {
    ensure_chrome(&webview)?;
    let settings = state.settings.read().map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
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
    let settings = settings.sanitized();

    {
        let mut current = state.settings.write().map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        *current = settings.clone();
    }
    state.save_settings()?;

    let capacity = settings.webview_capacity;
    mutate(&app, &state, |browser| Ok(browser.set_capacity(capacity)))?;

    SettingsChanged(settings.clone()).emit(&app).map_err(HakuError::from)?;
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
            let settings = state.settings.read().map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
            resolve_target(&url, &settings.search_url)
        }
        None => {
            let settings = state.settings.read().map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
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
    mutate(&app, &state, |browser| browser.close_tab(id))
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
        let settings = state.settings.read().map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
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
        let browser = state.browser.read().map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
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

/// Records a visit, tolerating a failure rather than breaking navigation.
fn record_visit(state: &AppState, url: &str, title: &str) {
    let Ok(history) = state.history.lock() else { return };
    let _ = history.record(url, title, now_ms() as i64);
}

#[tauri::command]
#[specta::specta]
pub fn recent_history(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    limit: u32,
) -> Result<Vec<HistoryEntry>> {
    ensure_chrome(&webview)?;
    let history = state.history.lock().map_err(|_| HakuError::Storage("history lock poisoned".into()))?;
    history.recent(limit as usize)
}

#[tauri::command]
#[specta::specta]
pub fn clear_history(webview: tauri::Webview, state: State<'_, AppState>) -> Result<()> {
    ensure_chrome(&webview)?;
    let history = state.history.lock().map_err(|_| HakuError::Storage("history lock poisoned".into()))?;
    history.clear()
}

/// Releases the webviews of pinned tabs that have gone quiet.
///
/// Driven by the interface on a timer rather than a background thread, so the
/// policy runs only while there is someone to see the result.
#[tauri::command]
#[specta::specta]
pub async fn release_idle_tabs(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<BrowserState> {
    ensure_chrome(&webview)?;
    let idle_after = {
        let settings = state.settings.read().map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        settings.idle_release_ms
    };
    mutate(&app, &state, |browser| Ok(browser.release_idle_fixed(now_ms(), idle_after)))
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
        let browser = state.browser.read().map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        browser.tab(id)?.slot()
    };
    if let Some(target) = slot.and_then(|slot| app.get_webview(&slot.label())) {
        target.open_devtools();
    }
    Ok(())
}
