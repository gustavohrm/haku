pub mod inject;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::Duration;

use tauri::{LogicalPosition, LogicalSize, Manager, WebviewUrl};

use crate::browser::{Effect, BLANK_URL};
use crate::error::{HakuError, Result};
use crate::model::{PageState, Placement, Scroll, SlotId, TabId, WindowId, WindowKind, WindowRequestId};
use crate::platform::{self, KeySink, PageSignal};
use crate::storage::WindowBounds;

/// Smallest a browser window may be made, in logical pixels.
const MIN_WINDOW_SIZE: (f64, f64) = (640.0, 480.0);

/// Smallest a popup may be made, in logical pixels. Popups are often small
/// on purpose, so they get far less room than a browser window.
const MIN_POPUP_SIZE: (f64, f64) = (240.0, 160.0);

/// What a popup's page is given when it asks for no size, in logical pixels.
const DEFAULT_POPUP_SIZE: (f64, f64) = (500.0, 600.0);

/// Height of a popup's own bar above its page, in logical pixels. A popup
/// asks for the size of its page, so the window is made this much taller.
/// The interface draws the bar at this height. Only the window's first size
/// depends on the two agreeing: the page is placed where the interface
/// reports it.
const POPUP_BAR_HEIGHT: f64 = 34.0;

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

/// Where each window's content webviews belong, in logical pixels.
///
/// An internal page reports no viewport, because the chrome covers the whole
/// window while one is open. Content webviews still belong at the last real
/// rectangle, so remembering it keeps a webview created while an internal
/// page is showing from being built at zero size and staying invisible.
#[derive(Default)]
pub struct Viewports(RwLock<HashMap<WindowId, Viewport>>);

impl Viewports {
    /// Zero until the window's interface has reported a layout, or one was
    /// guessed for it as it opened.
    pub fn get(&self, window: WindowId) -> Viewport {
        self.0
            .read()
            .ok()
            .and_then(|viewports| viewports.get(&window).copied())
            .unwrap_or_default()
    }

    pub fn set(&self, window: WindowId, viewport: Viewport) {
        if let Ok(mut viewports) = self.0.write() {
            viewports.insert(window, viewport);
        }
    }

    fn remove(&self, window: WindowId) {
        if let Ok(mut viewports) = self.0.write() {
            viewports.remove(&window);
        }
    }

    /// Any window's viewport, as a guess at a new browser window's: they all
    /// draw the same interface.
    fn any(&self) -> Viewport {
        self.0
            .read()
            .ok()
            .and_then(|viewports| viewports.values().next().copied())
            .unwrap_or_default()
    }
}

/// Receives what content webviews report, tagged with the slot they occupy.
///
/// Supplied by the caller so this module stays free of application state: it
/// knows how to drive webviews, not what to do with what they report. Called on
/// the UI thread.
pub type PageObserver<R> = Arc<dyn Fn(&tauri::AppHandle<R>, SlotId, PageSignal) + Send + Sync>;

/// Makes the sink that receives the shortcuts pressed while a window's chrome
/// has focus.
pub type KeyObserver<R> = Arc<dyn Fn(&tauri::AppHandle<R>, WindowId) -> KeySink + Send + Sync>;

/// What the caller attaches to the webviews this module creates.
pub struct Hooks<R: tauri::Runtime> {
    pub page: PageObserver<R>,
    pub keys: KeyObserver<R>,
}

/// How a window is first put on screen.
#[derive(Clone, Copy, Debug)]
pub enum Frame {
    /// Filling the screen, as a browser window first opens.
    Maximized,
    /// Where it was before, as a restored window opens.
    Bounds(WindowBounds),
    /// Where and how large its page asked for.
    Popup(Placement),
}

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

/// The chrome of the window opened first among those still open, for what
/// the engine does once for every webview: installing extensions, measuring
/// memory, watching service workers. Every chrome shares the one profile.
///
/// # Errors
/// Returns [`HakuError::WindowMissing`] when no window is open.
pub fn chrome<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<tauri::Webview<R>> {
    app.webviews()
        .into_values()
        .filter_map(|webview| Some((WindowId::from_label(webview.label())?, webview)))
        .min_by_key(|(window, _)| *window)
        .map(|(_, webview)| webview)
        .ok_or_else(|| HakuError::WindowMissing("no browser window".into()))
}

/// The webview that draws a window's interface.
///
/// # Errors
/// Returns [`HakuError::WindowMissing`] when the window is not open.
pub fn chrome_of<R: tauri::Runtime>(app: &tauri::AppHandle<R>, window: WindowId) -> Result<tauri::Webview<R>> {
    app.get_webview(&window.label())
        .ok_or_else(|| HakuError::WindowMissing(window.label()))
}

fn native_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>, window: WindowId) -> Result<tauri::Window<R>> {
    app.get_window(&window.label())
        .ok_or_else(|| HakuError::WindowMissing(window.label()))
}

/// Builds a browser window and the chrome that fills it.
///
/// Safe on the main thread, which is where startup builds the restored
/// windows. The chrome is not ready for pages until [`prepare_chrome`] runs.
///
/// # Errors
/// Propagates Tauri's failure to create the window.
pub fn build_window<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    window: WindowId,
    kind: WindowKind,
    frame: Frame,
) -> Result<tauri::WebviewWindow<R>> {
    let min = match kind {
        WindowKind::Normal => MIN_WINDOW_SIZE,
        WindowKind::Popup => MIN_POPUP_SIZE,
    };
    let builder = tauri::WebviewWindowBuilder::new(app, window.label(), WebviewUrl::default())
        .title("Haku")
        .decorations(false)
        .min_inner_size(min.0, min.1)
        // Every webview sharing a profile must agree on this, content
        // webviews included; see `create_slot`.
        .browser_extensions_enabled(true);
    let builder = match frame {
        Frame::Maximized => builder.maximized(true),
        Frame::Bounds(bounds) => builder
            .position(bounds.x, bounds.y)
            .inner_size(bounds.width, bounds.height)
            .maximized(bounds.maximized),
        Frame::Popup(placement) => {
            let (width, height) = placement.size.unwrap_or(DEFAULT_POPUP_SIZE);
            let builder = builder.inner_size(width, height + POPUP_BAR_HEIGHT);
            match placement.position {
                Some((x, y)) => builder.position(x, y),
                None => builder.center(),
            }
        }
    };
    Ok(builder.build()?)
}

/// Lifts a window's chrome over the page content to come and starts taking
/// its shortcuts.
///
/// Must not be called on the main thread: both wait on it.
///
/// # Errors
/// Returns [`HakuError::WindowMissing`] when the window has closed, and the
/// platform's failure to attach to it.
pub fn prepare_chrome<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    window: WindowId,
    keys: &KeyObserver<R>,
) -> Result<()> {
    let chrome = chrome_of(app, window)?;
    platform::raise_chrome(&chrome)?;
    platform::intercept_keys(&chrome, keys(app, window))
}

/// The viewport a window that has not laid out yet is given, so the first
/// page put in it is not built at zero size: a popup's page fills it below
/// its bar, and a browser window's sits where every other one's does.
fn guessed_viewport(frame: Frame, viewports: &Viewports) -> Viewport {
    match frame {
        Frame::Popup(placement) => {
            let (width, height) = placement.size.unwrap_or(DEFAULT_POPUP_SIZE);
            Viewport {
                x: 0.0,
                y: POPUP_BAR_HEIGHT,
                width,
                height,
            }
        }
        Frame::Maximized | Frame::Bounds(_) => viewports.any(),
    }
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
/// Returns [`HakuError::WindowMissing`] when a window an effect puts a
/// webview in is gone, and propagates Tauri failures from the individual
/// operations.
pub fn apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    effects: &[Effect],
    viewports: &Viewports,
    hooks: &Hooks<R>,
) -> Result<Vec<Departure>> {
    let mut departures = Vec::new();
    for effect in effects {
        match effect {
            Effect::EnsureSlot { slot, url, window } => {
                set_pending(*slot, None);
                renavigate(*slot);
                ensure_slot(app, *slot, url, *window, viewports.get(*window), &hooks.page)?;
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
            Effect::Adopt {
                slot,
                request,
                url,
                window,
            } => {
                set_pending(*slot, None);
                renavigate(*slot);
                adopt(app, *slot, *request, url, *window, viewports.get(*window), &hooks.page)?;
            }
            Effect::Move { slot, window } => move_slot(app, *slot, *window, viewports.get(*window))?,
            Effect::OpenWindow {
                window,
                kind,
                placement,
            } => {
                let frame = match kind {
                    WindowKind::Normal => Frame::Maximized,
                    WindowKind::Popup => Frame::Popup(*placement),
                };
                viewports.set(*window, guessed_viewport(frame, viewports));
                build_window(app, *window, *kind, frame)?;
                prepare_chrome(app, *window, &hooks.keys)?;
            }
            Effect::CloseWindow { window } => {
                viewports.remove(*window);
                if let Some(native) = app.get_window(&window.label()) {
                    native.destroy()?;
                }
            }
            // Bringing a window forward is a courtesy; failing to does not
            // undo what opened in it.
            Effect::FocusWindow { window } => {
                if let Ok(native) = native_window(app, *window) {
                    let _ = native.unminimize();
                    let _ = native.set_focus();
                }
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
    window: WindowId,
    viewport: Viewport,
    observer: &PageObserver<R>,
) -> Result<()> {
    if app.get_webview(&slot.label()).is_none() {
        create_slot(app, slot, window, viewport, observer)?;
    }
    navigate(app, slot, url)
}

/// Moves a slot's webview into another window, whose chrome is then lifted
/// back over it, as it is when a slot is created there.
fn move_slot<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    slot: SlotId,
    window: WindowId,
    viewport: Viewport,
) -> Result<()> {
    let Some(webview) = app.get_webview(&slot.label()) else {
        return Ok(());
    };
    webview.reparent(&native_window(app, window)?)?;
    webview.set_bounds(rect_of(viewport))?;
    platform::raise_chrome(&chrome_of(app, window)?)
}

/// Creates the slot's webview and hands it to the page that asked for a new
/// window, which then loads its page into it. Loads `url` itself when the
/// request is no longer waiting or the webview could not be handed to it.
fn adopt<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    slot: SlotId,
    request: WindowRequestId,
    url: &str,
    window: WindowId,
    viewport: Viewport,
    observer: &PageObserver<R>,
) -> Result<()> {
    if app.get_webview(&slot.label()).is_some() {
        // A webview that loaded anything cannot be handed over.
        platform::refuse_window(app, request)?;
        return navigate(app, slot, url);
    }
    let webview = create_slot(app, slot, window, viewport, observer)?;
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
    window: WindowId,
    viewport: Viewport,
    observer: &PageObserver<R>,
) -> Result<tauri::Webview<R>> {
    let chrome = chrome_of(app, window)?;
    let window = native_window(app, window)?;

    // Created on a blank page and navigated only once it is being observed, so
    // the first commit of the real page cannot slip past before the observer is
    // attached.
    let builder = tauri::webview::WebviewBuilder::new(slot.label(), WebviewUrl::External(parse_url(BLANK_URL)?))
        .user_agent(USER_AGENT)
        // Every webview sharing a profile must agree on this, the chrome
        // included; see `build_window`.
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
    platform::raise_chrome(&chrome)?;

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
