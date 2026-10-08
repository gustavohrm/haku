pub mod inject;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use tauri::{LogicalPosition, LogicalSize, Manager, WebviewUrl};

use crate::browser::{Effect, BLANK_URL};
use crate::error::{HakuError, Result};
use crate::model::{PageState, Scroll, SlotId, TabId, WindowRequestId};
use crate::platform::{self, PageSignal};

/// Label of the webview that renders Haku's own interface.
///
/// It is the window's original webview, so it shares the window label.
pub const CHROME_LABEL: &str = "main";

pub const MAIN_WINDOW_LABEL: &str = "main";

/// Desktop user agent. The system webview would otherwise announce itself with
/// an Edge/WebView2 string that some sites treat as an unsupported browser.
const USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ",
    "(KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36"
);

/// Where content webviews sit inside the window, in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct Viewport {
    #[specta(type = specta_typescript::Number)]
    pub x: f64,
    #[specta(type = specta_typescript::Number)]
    pub y: f64,
    #[specta(type = specta_typescript::Number)]
    pub width: f64,
    #[specta(type = specta_typescript::Number)]
    pub height: f64,
}

/// Receives what content webviews report, tagged with the slot they occupy.
///
/// Supplied by the caller so this module stays free of application state: it
/// knows how to drive webviews, not what to do with what they report. Called on
/// the UI thread.
pub type PageObserver<R> = Arc<dyn Fn(&tauri::AppHandle<R>, SlotId, PageSignal) + Send + Sync>;

/// Longest a page is waited on when it is read and captured as it is left,
/// which is also the most a tab switch can be held up by it.
pub const LEAVE_TIMEOUT: Duration = Duration::from_millis(150);

/// What a page held, read as it was left or on a tick. `None` means it could
/// not be read.
pub type PageReading = (TabId, Option<PageState>);

/// What a page held as it was left, and what it showed if it was captured.
pub struct Departure {
    pub tab: TabId,
    /// Nothing when the page could not be read.
    pub state: Option<PageState>,
    /// A JPEG of the page, when a capture was asked for and succeeded.
    pub preview: Option<Vec<u8>>,
}

/// Scroll and draft waiting for a slot's next document to load.
struct PendingRestore {
    url: String,
    scroll: Scroll,
    height: Option<f64>,
    draft: Option<String>,
}

/// Restores waiting per slot. Set by [`Effect::RestoreState`], taken when the
/// document loads, and dropped whenever the slot is navigated elsewhere, so a
/// restore never lands on another tab's page.
static PENDING_RESTORES: LazyLock<Mutex<HashMap<SlotId, PendingRestore>>> = LazyLock::new(Mutex::default);

/// Slots whose document was handed a scroll to restore, until it finishes
/// loading. The page holds itself at that scroll while it is still growing,
/// so the end of its load is reported only once it says the scroll has
/// settled, keeping the cover over the page until it is where it was.
static SCROLLING: LazyLock<Mutex<HashSet<SlotId>>> = LazyLock::new(Mutex::default);

/// How many times each slot has been navigated by Haku, so a wait on one page
/// can tell that the slot has since moved on to another.
static NAVIGATIONS: LazyLock<Mutex<HashMap<SlotId, u64>>> = LazyLock::new(Mutex::default);

/// How often a settling page is asked whether its scroll has settled.
const SETTLE_POLL: Duration = Duration::from_millis(50);

/// Longest the end of a load is held back for a settling page: the page's
/// own limit, and a margin for asking it.
const SETTLE_WAIT: Duration = Duration::from_millis(inject::SCROLL_SETTLE_LIMIT_MS as u64 + 500);

fn set_scrolling(slot: SlotId, scrolling: bool) -> bool {
    SCROLLING.lock().is_ok_and(|mut slots| {
        if scrolling {
            slots.insert(slot)
        } else {
            slots.remove(&slot)
        }
    })
}

/// Marks a slot as navigated elsewhere, ending any wait on its page.
fn renavigate(slot: SlotId) {
    set_scrolling(slot, false);
    if let Ok(mut navigations) = NAVIGATIONS.lock() {
        *navigations.entry(slot).or_default() += 1;
    }
}

fn navigations(slot: SlotId) -> u64 {
    NAVIGATIONS
        .lock()
        .map(|navigations| navigations.get(&slot).copied().unwrap_or_default())
        .unwrap_or_default()
}

/// Waits, off the UI thread, until the slot's page says its restored scroll
/// has settled, the page stops answering, or the wait runs out.
///
/// @returns Whether the slot still shows the same page, which is when its
///   load is worth reporting.
fn wait_for_scroll<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId) -> bool {
    let navigation = navigations(slot);
    let started = std::time::Instant::now();
    while started.elapsed() < SETTLE_WAIT {
        if navigations(slot) != navigation {
            return false;
        }
        let Some(webview) = app.get_webview(&slot.label()) else {
            return false;
        };
        match platform::evaluate(&webview, &inject::scroll_settled_expression(), SETTLE_POLL * 4) {
            Some(answer) if answer != "false" => break,
            None => break,
            Some(_) => std::thread::sleep(SETTLE_POLL),
        }
    }
    navigations(slot) == navigation
}

fn set_pending(slot: SlotId, restore: Option<PendingRestore>) {
    if let Ok(mut pending) = PENDING_RESTORES.lock() {
        match restore {
            Some(restore) => pending.insert(slot, restore),
            None => pending.remove(&slot),
        };
    }
}

fn rect_of(viewport: Viewport) -> tauri::Rect {
    tauri::Rect {
        position: tauri::Position::Logical(LogicalPosition::new(viewport.x, viewport.y)),
        size: tauri::Size::Logical(LogicalSize::new(viewport.width, viewport.height)),
    }
}

pub fn chrome<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<tauri::Webview<R>> {
    app.get_webview(CHROME_LABEL)
        .ok_or_else(|| HakuError::WindowMissing(CHROME_LABEL.into()))
}

/// Applies the effects [`crate::browser::Browser`] produced to real webviews.
///
/// Reading a page that is being left, and capturing it when asked, waits for
/// the page, up to [`LEAVE_TIMEOUT`], before the effect that leaves it is
/// applied. Commands that apply effects run off the UI thread, so the wait
/// never blocks it.
///
/// @returns What each page that was left held and showed, for
///   [`crate::browser::Browser::report_state`] and the preview store.
///
/// # Errors
/// Returns [`HakuError::WindowMissing`] when the main window is gone, and
/// propagates Tauri failures from the individual operations.
pub fn apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    effects: &[Effect],
    viewport: Viewport,
    observer: &PageObserver<R>,
) -> Result<Vec<Departure>> {
    let mut departures = Vec::new();
    for effect in effects {
        match effect {
            Effect::EnsureSlot { slot, url } => {
                set_pending(*slot, None);
                renavigate(*slot);
                ensure_slot(app, *slot, url, viewport, observer)?;
            }
            Effect::Blank { slot } => {
                set_pending(*slot, None);
                renavigate(*slot);
                navigate(app, *slot, BLANK_URL)?;
            }
            Effect::Destroy { slot } => {
                set_pending(*slot, None);
                renavigate(*slot);
                destroy(app, *slot)?;
            }
            Effect::Show { slot } => set_visible(app, *slot, true)?,
            Effect::Hide { slot } => set_visible(app, *slot, false)?,
            Effect::Reload { slot } => reload(app, *slot)?,
            // A page that fails to freeze keeps running, which costs memory but
            // nothing else, so it must not stop the effects after it.
            Effect::Freeze { slot } => {
                if let Some(webview) = app.get_webview(&slot.label()) {
                    let _ = platform::freeze(&webview);
                }
            }
            Effect::Resume { slot } => {
                if let Some(webview) = app.get_webview(&slot.label()) {
                    let _ = platform::resume(&webview);
                }
            }
            Effect::Leave { slot, tab, capture } => departures.push(leave_page(app, *slot, *tab, *capture)),
            Effect::RestoreState {
                slot,
                url,
                scroll,
                height,
                draft,
            } => set_pending(
                *slot,
                Some(PendingRestore {
                    url: url.clone(),
                    scroll: *scroll,
                    height: *height,
                    draft: draft.clone(),
                }),
            ),
            Effect::AnswerDialog { id, answer } => platform::answer_dialog(app, *id, answer.clone())?,
            Effect::Adopt { slot, request, url } => {
                set_pending(*slot, None);
                renavigate(*slot);
                adopt(app, *slot, *request, url, viewport, observer)?;
            }
        }
    }
    Ok(departures)
}

fn leave_page<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, tab: TabId, capture: bool) -> Departure {
    let Some(webview) = app.get_webview(&slot.label()) else {
        return Departure {
            tab,
            state: None,
            preview: None,
        };
    };
    let (json, preview) =
        platform::leave_page(&webview, &inject::page_state_drain_expression(), capture, LEAVE_TIMEOUT);
    Departure {
        tab,
        state: json.as_deref().and_then(inject::parse_page_state),
        preview,
    }
}

/// Reads what a slot's page holds, waiting at most `timeout`.
///
/// Must not be called on the UI thread, where the reading has to run.
///
/// @returns The reading, or nothing when the page has no record, does not
///   answer in time, or the slot has no webview.
pub fn read_page<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, timeout: Duration) -> Option<PageState> {
    let webview = app.get_webview(&slot.label())?;
    let json = platform::evaluate(&webview, &inject::page_state_drain_expression(), timeout)?;
    inject::parse_page_state(&json)
}

/// Hands a freshly loaded document the restore waiting for its slot.
///
/// The blank page a slot is created or parked on is not the document the
/// restore is for, and leaves it waiting.
fn restore_pending<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, url: &str) {
    if url == BLANK_URL {
        return;
    }
    let Some(restore) = PENDING_RESTORES
        .lock()
        .ok()
        .and_then(|mut pending| pending.remove(&slot))
    else {
        return;
    };
    if restore.scroll != Scroll::default() {
        set_scrolling(slot, true);
    }
    if let Some(webview) = app.get_webview(&slot.label()) {
        let _ = webview.eval(inject::restore_expression(
            &restore.url,
            restore.scroll,
            restore.height,
            restore.draft.as_deref(),
        ));
    }
}

/// Creates the slot's webview if it does not exist yet, otherwise navigates it.
fn ensure_slot<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    slot: SlotId,
    url: &str,
    viewport: Viewport,
    observer: &PageObserver<R>,
) -> Result<()> {
    if app.get_webview(&slot.label()).is_none() {
        create_slot(app, slot, viewport, observer)?;
    }
    navigate(app, slot, url)
}

/// Creates the slot's webview and hands it to the page that asked for a new
/// window, which then loads its page into it. Loads `url` itself when the
/// request is no longer waiting or the webview could not be handed to it.
fn adopt<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    slot: SlotId,
    request: WindowRequestId,
    url: &str,
    viewport: Viewport,
    observer: &PageObserver<R>,
) -> Result<()> {
    if app.get_webview(&slot.label()).is_some() {
        // A webview that loaded anything cannot be handed over.
        platform::refuse_window(app, request)?;
        return navigate(app, slot, url);
    }
    let webview = create_slot(app, slot, viewport, observer)?;
    // A request that could not be handed the webview, for whatever reason,
    // still gets its page: the tab loads it, unconnected.
    if platform::adopt_window(&webview, request).unwrap_or(false) {
        return Ok(());
    }
    navigate(app, slot, url)
}

/// Creates a slot's webview on a blank page, observed. It has loaded nothing,
/// so it can still be handed to a page that asked for a new window.
fn create_slot<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    slot: SlotId,
    viewport: Viewport,
    observer: &PageObserver<R>,
) -> Result<tauri::Webview<R>> {
    let window = app
        .get_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| HakuError::WindowMissing(MAIN_WINDOW_LABEL.into()))?;

    // Created on a blank page and navigated only once it is being observed, so
    // the first commit of the real page cannot slip past before the observer is
    // attached.
    let builder = tauri::webview::WebviewBuilder::new(slot.label(), WebviewUrl::External(parse_url(BLANK_URL)?))
        .user_agent(USER_AGENT)
        // Every webview sharing a profile must agree on this, the chrome
        // included; see `tauri.conf.json`.
        .browser_extensions_enabled(true)
        .devtools(true)
        .initialization_script(inject::navigation_log_script())
        .initialization_script(inject::page_state_script());

    let webview = window.add_child(
        builder,
        LogicalPosition::new(viewport.x, viewport.y),
        LogicalSize::new(viewport.width, viewport.height),
    )?;

    // A new child webview is placed above everything, including the chrome, so
    // the chrome has to be lifted back over it. Without this the interface
    // disappears behind the page the moment a slot is created.
    platform::raise_chrome(&chrome(app)?)?;

    // What the page shows is observed from Rust rather than reported by the
    // page, so a remote origin never needs a channel into the application.
    let sink = {
        let observer = observer.clone();
        let app = app.clone();
        Arc::new(move |signal| match signal {
            PageSignal::Loaded { url } => {
                restore_pending(&app, slot, &url);
                observer(&app, slot, PageSignal::Loaded { url });
            }
            PageSignal::Completed { url } if set_scrolling(slot, false) => {
                let (app, observer) = (app.clone(), observer.clone());
                std::thread::spawn(move || {
                    if wait_for_scroll(&app, slot) {
                        observer(&app, slot, PageSignal::Completed { url });
                    }
                });
            }
            signal => observer(&app, slot, signal),
        })
    };
    platform::observe_page(&webview, sink)?;
    Ok(webview)
}

fn navigate<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, url: &str) -> Result<()> {
    let Some(webview) = app.get_webview(&slot.label()) else {
        return Ok(());
    };
    webview.navigate(parse_url(url)?)?;
    Ok(())
}

fn reload<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId) -> Result<()> {
    if let Some(webview) = app.get_webview(&slot.label()) {
        webview.reload()?;
    }
    Ok(())
}

fn destroy<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId) -> Result<()> {
    if let Some(webview) = app.get_webview(&slot.label()) {
        webview.close()?;
    }
    Ok(())
}

fn set_visible<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, visible: bool) -> Result<()> {
    let Some(webview) = app.get_webview(&slot.label()) else {
        return Ok(());
    };
    if visible {
        webview.show()?;
    } else {
        webview.hide()?;
    }
    Ok(())
}

/// Moves every content webview to the viewport rectangle the chrome reports.
pub fn set_bounds<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slots: &[SlotId], viewport: Viewport) -> Result<()> {
    for slot in slots {
        if let Some(webview) = app.get_webview(&slot.label()) {
            webview.set_bounds(rect_of(viewport))?;
        }
    }
    Ok(())
}

fn parse_url(url: &str) -> Result<tauri::Url> {
    tauri::Url::parse(url).map_err(|error| HakuError::InvalidUrl(format!("{url}: {error}")))
}
