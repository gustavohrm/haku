pub mod inject;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::Duration;

use tauri::{LogicalPosition, LogicalSize, Manager, WebviewUrl};

use crate::browser::{Browser, Effect, ViewportAssignment, ViewportTarget, BLANK_URL, INITIAL_VIEWPORT};
use crate::chrome::Layout;
use crate::error::{HakuError, Result};
use crate::model::{
    popup_origin, popup_size, PageState, Placement, ScreenRect, Scroll, SlotId, TabId, WindowId, WindowKind,
    WindowRequestId, POPUP_MIN_SIZE,
};
use crate::platform::{self, FailureSink, KeySink, PageSignal, PageSink};
use crate::storage::WindowBounds;
use inject::ExtensionView;

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

/// Last noncollapsed bounds, plus readiness for a particular layout revision.
struct MeasuredViewport {
    bounds: Viewport,
    kind: WindowKind,
    revision: Option<u64>,
    measurable: bool,
}

/// Pane geometry keyed by (window, stable window-local viewport id).
/// Covered/collapsed reports keep the last real size for background creation.
#[derive(Default)]
pub struct Viewports(RwLock<HashMap<(WindowId, String), MeasuredViewport>>);

impl Viewports {
    /// Zero until measured or guessed when the window opens.
    pub fn get(&self, window: WindowId, id: &str) -> Viewport {
        self.0
            .read()
            .ok()
            .and_then(|viewports| viewports.get(&(window, id.into())).map(|pane| pane.bounds))
            .unwrap_or_default()
    }

    /// Resolves each slot's bounds in the reporting window.
    pub fn slot_bounds(&self, browser: &Browser, window: WindowId) -> Vec<(SlotId, Viewport)> {
        let Some(current) = browser.state_in(window) else {
            return Vec::new();
        };
        let placements = browser.slot_viewports();
        browser
            .slots_in(window)
            .into_iter()
            .map(|slot| {
                // Background slots retain the focused pane's last measured bounds.
                let id = placements
                    .get(&slot)
                    .map_or(current.focused_viewport.as_str(), |target| target.id.as_str());
                (slot, self.get(window, id))
            })
            .collect()
    }

    /// Bounds for effect application, including hidden slots in the focused pane.
    pub fn all_slot_bounds(&self, browser: &Browser) -> HashMap<SlotId, Viewport> {
        browser
            .window_ids()
            .into_iter()
            .flat_map(|window| self.slot_bounds(browser, window))
            .collect()
    }

    /// Accepts a complete layout only for the current layout revision.
    ///
    /// # Errors
    /// Returns the layout validation error, or [`HakuError::Storage`] if locked.
    pub fn record(
        &self,
        window: WindowId,
        layout: &Layout,
        assignments: &[ViewportAssignment],
        revision: u64,
    ) -> Result<bool> {
        if layout.revision != revision {
            return Ok(false);
        }
        layout.validate(assignments)?;
        let mut viewports = self
            .0
            .write()
            .map_err(|_| HakuError::Storage("viewport lock poisoned".into()))?;
        for pane in &layout.viewports {
            let known = viewports.entry((window, pane.id.clone())).or_insert(MeasuredViewport {
                bounds: Viewport::default(),
                kind: WindowKind::Normal,
                revision: None,
                measurable: false,
            });
            let measurable = pane.rect.width >= 1.0 && pane.rect.height >= 1.0;
            if measurable {
                known.bounds = pane.rect;
            }
            known.revision = Some(revision);
            known.measurable = measurable;
        }
        Ok(true)
    }

    /// A slot may show only after its current assignment has usable geometry.
    pub fn can_show(&self, target: &ViewportTarget) -> bool {
        self.0.read().is_ok_and(|viewports| {
            viewports
                .get(&(target.window, target.id.clone()))
                .is_some_and(|pane| pane.revision == Some(target.revision) && pane.measurable)
        })
    }

    fn open(&self, window: WindowId, kind: WindowKind, bounds: Viewport) {
        if let Ok(mut viewports) = self.0.write() {
            viewports.insert(
                (window, INITIAL_VIEWPORT.into()),
                MeasuredViewport {
                    bounds,
                    kind,
                    revision: None,
                    measurable: false,
                },
            );
        }
    }

    fn remove(&self, window: WindowId) {
        if let Ok(mut viewports) = self.0.write() {
            viewports.retain(|(owner, _), _| *owner != window);
        }
    }

    /// Any normal window's pane as an initial size guess; never a popup's.
    fn any(&self) -> Viewport {
        self.0
            .read()
            .ok()
            .and_then(|viewports| {
                viewports
                    .values()
                    .find(|pane| pane.kind == WindowKind::Normal)
                    .map(|pane| pane.bounds)
            })
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

/// Makes the sink that receives a window chrome's process failures.
pub type FailureObserver<R> = Arc<dyn Fn(&tauri::AppHandle<R>, WindowId) -> FailureSink + Send + Sync>;

/// What the caller attaches to the webviews this module creates.
pub struct Hooks<R: tauri::Runtime> {
    pub page: PageObserver<R>,
    pub keys: KeyObserver<R>,
    pub failures: FailureObserver<R>,
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
        // Every webview sharing a profile must agree on these, content
        // webviews included; see `create_slot`.
        .browser_extensions_enabled(true)
        // The engine's own form autofill stays off, so it stores nothing a
        // page is given. Its password saving is off by default and never
        // turned on: a password-manager extension is the route for both.
        .general_autofill_enabled(false);
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

/// Lifts a window's chrome over the page content to come, starts taking its
/// shortcuts, and starts reporting its process failures.
///
/// Must not be called on the main thread: both wait on it.
///
/// # Errors
/// Returns [`HakuError::WindowMissing`] when the window has closed, and the
/// platform's failure to attach to it.
pub fn prepare_chrome<R: tauri::Runtime>(app: &tauri::AppHandle<R>, window: WindowId, hooks: &Hooks<R>) -> Result<()> {
    let chrome = chrome_of(app, window)?;
    platform::raise_chrome(&chrome)?;
    // Each is attached even when another fails: a window without shortcuts
    // should still recover from a crash, and the other way round.
    let keys = platform::intercept_keys(&chrome, (hooks.keys)(app, window));
    let failures = platform::watch_failures(&chrome, (hooks.failures)(app, window));
    let clipboard = platform::allow_clipboard_read(&chrome);
    keys.and(failures).and(clipboard)
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
///   [`crate::browser::Browser::report_state`] and the preview store, even
///   when an effect failed; slots whose creation or adoption failed, for model
///   repair; and the first failure, once every effect has
///   been tried: [`HakuError::WindowMissing`] when a window an effect puts a
///   webview in is gone, or a Tauri failure from an individual operation.
pub fn apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    effects: &[Effect],
    viewports: &Viewports,
    hooks: &Hooks<R>,
    placements: &HashMap<SlotId, ViewportTarget>,
    bounds: &HashMap<SlotId, Viewport>,
) -> (Vec<Departure>, Vec<SlotId>, Result<()>) {
    // Newly opened windows receive their initial guess in OpenWindow below.
    let bounds_for = |slot: &SlotId, window| {
        bounds
            .get(slot)
            .copied()
            .filter(|rect| rect.width >= 1.0 && rect.height >= 1.0)
            .unwrap_or_else(|| {
                placements.get(slot).map_or_else(
                    || viewports.get(window, INITIAL_VIEWPORT),
                    |target| viewports.get(window, &target.id),
                )
            })
    };
    let mut departures = Vec::new();
    let (failed_slots, applied) = each_effect(effects, |effect| {
        match effect {
            Effect::EnsureSlot { slot, url, window } => {
                set_pending(*slot, None);
                renavigate(*slot);
                if !placements.get(slot).is_some_and(|target| viewports.can_show(target)) {
                    set_visible(app, *slot, false)?;
                }
                ensure_slot(app, *slot, url, *window, bounds_for(slot, *window), &hooks.page)?;
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
            Effect::Show { slot } => {
                let target = placements.get(slot);
                let visible = target.is_some_and(|target| viewports.can_show(target));
                if let Some(target) = target.filter(|_| visible) {
                    set_bounds(app, *slot, viewports.get(target.window, &target.id))?;
                }
                set_visible(app, *slot, visible)?;
            }
            Effect::Hide { slot } => set_visible(app, *slot, false)?,
            Effect::FocusPane { window, slot } => {
                let page = slot.and_then(|slot| {
                    placements.get(&slot).filter(|target| viewports.can_show(target))?;
                    app.get_webview(&slot.label())
                });
                match page {
                    Some(page) => page.set_focus()?,
                    None => chrome_of(app, *window)?.set_focus()?,
                }
            }
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
                adopt(
                    app,
                    *slot,
                    *request,
                    url,
                    *window,
                    bounds_for(slot, *window),
                    &hooks.page,
                )?;
            }
            Effect::Move { slot, window } => {
                if !placements.get(slot).is_some_and(|target| viewports.can_show(target)) {
                    set_visible(app, *slot, false)?;
                }
                move_slot(app, *slot, *window, bounds_for(slot, *window))?;
            }
            Effect::OpenWindow {
                window,
                kind,
                placement,
            } => {
                let frame = match kind {
                    WindowKind::Normal => Frame::Maximized,
                    WindowKind::Popup => Frame::Popup(*placement),
                };
                viewports.open(*window, *kind, guessed_viewport(frame, viewports));
                build_window(app, *window, *kind, frame)?;
                prepare_chrome(app, *window, hooks)?;
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
        Ok(())
    });
    (departures, failed_slots, applied)
}

/// Runs `apply` on every effect, even after one fails, and returns failed slot
/// bindings alongside the first failure.
///
/// The browser recorded every effect as done when it decided them, so one
/// that fails must not keep the rest from happening: a dialog answered after
/// a webview that could not be moved would otherwise be left open, its page
/// paused for good.
fn each_effect(effects: &[Effect], mut apply: impl FnMut(&Effect) -> Result<()>) -> (Vec<SlotId>, Result<()>) {
    let mut failure = None;
    let mut failed_slots = Vec::new();
    for effect in effects {
        if let Err(error) = apply(effect) {
            if let Effect::EnsureSlot { slot, .. } | Effect::Adopt { slot, .. } = effect {
                failed_slots.push(*slot);
            }
            failure.get_or_insert(error);
        }
    }
    (failed_slots, failure.map_or(Ok(()), Err))
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
        // Every webview sharing a profile must agree on these, the chrome
        // included; see `build_window`.
        .browser_extensions_enabled(true)
        .general_autofill_enabled(false)
        .focused(false)
        .devtools(true)
        .initialization_script(inject::navigation_log_script())
        .initialization_script(inject::page_state_script())
        .initialization_script(inject::extension_page_script(ExtensionView::Tab));

    let webview = window.add_child(
        builder,
        LogicalPosition::new(viewport.x, viewport.y),
        LogicalSize::new(viewport.width, viewport.height),
    )?;

    webview.hide()?;

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

/// Each held-back extension page by slot, with its URL and the slot's
/// [`LOADS`] count from when it was held.
type Waiting = HashMap<SlotId, (String, u64)>;

/// Extension pages a slot was sent to before the extensions folder was
/// installed, or `None` once it has been.
///
/// At startup, restored tabs load as soon as their webviews exist, while the
/// extensions are installed later, off the main thread. An extension page
/// loaded before its extension fails, and the failure replaces the page in its
/// tab's history, so it is lost for good. Such a load waits here instead, with
/// the slot on the blank page, until [`extensions_installed`] sends it on.
static WAITING_FOR_EXTENSIONS: LazyLock<Mutex<Option<Waiting>>> = LazyLock::new(|| Mutex::new(Some(HashMap::new())));

/// How many times each slot has been sent somewhere or destroyed, so a load
/// held back for the extensions can tell the slot has since moved on.
static LOADS: LazyLock<Mutex<HashMap<SlotId, u64>>> = LazyLock::new(Mutex::default);

/// Counts a new destination for the slot, and returns the count.
fn next_load(slot: SlotId) -> u64 {
    LOADS.lock().map_or(0, |mut loads| {
        let count = loads.entry(slot).or_default();
        *count += 1;
        *count
    })
}

fn loads(slot: SlotId) -> u64 {
    LOADS
        .lock()
        .map(|loads| loads.get(&slot).copied().unwrap_or_default())
        .unwrap_or_default()
}

const EXTENSION_SCHEME: &str = "chrome-extension://";

/// Loads the extension pages held back until the extensions folder was
/// installed, and lets later ones load at once. A slot sent elsewhere since
/// its page was held back keeps its newer destination.
pub fn extensions_installed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let waiting = WAITING_FOR_EXTENSIONS
        .lock()
        .ok()
        .and_then(|mut waiting| waiting.take())
        .unwrap_or_default();
    for (slot, (url, held_at)) in waiting {
        if loads(slot) == held_at {
            let _ = load(app, slot, &url);
        }
    }
}

/// Holds back a load of an extension page until the extensions are installed,
/// and forgets any held-back load the slot no longer wants.
///
/// @param held_at - The slot's [`LOADS`] count for this load.
/// @returns Whether the load was held back.
fn hold_until_extensions(slot: SlotId, url: Option<&str>, held_at: u64) -> bool {
    let Ok(mut waiting) = WAITING_FOR_EXTENSIONS.lock() else {
        return false;
    };
    let Some(waiting) = waiting.as_mut() else {
        return false;
    };
    match url {
        Some(url) if url.starts_with(EXTENSION_SCHEME) => {
            waiting.insert(slot, (url.to_owned(), held_at));
            true
        }
        _ => {
            waiting.remove(&slot);
            false
        }
    }
}

fn navigate<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, url: &str) -> Result<()> {
    let held_at = next_load(slot);
    if app.get_webview(&slot.label()).is_none() || hold_until_extensions(slot, Some(url), held_at) {
        return Ok(());
    }
    load(app, slot, url)
}

fn load<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, url: &str) -> Result<()> {
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
    hold_until_extensions(slot, None, next_load(slot));
    if let Some(webview) = app.get_webview(&slot.label()) {
        webview.close()?;
    }
    Ok(())
}

fn set_visible<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, visible: bool) -> Result<()> {
    let Some(webview) = app.get_webview(&slot.label()) else {
        return Ok(());
    };
    platform::set_visible(&webview, visible)
}

/// Positions a slot only after resolving its current pane assignment.
///
/// # Errors
/// Propagates a Tauri failure to position the webview.
pub fn set_bounds<R: tauri::Runtime>(app: &tauri::AppHandle<R>, slot: SlotId, viewport: Viewport) -> Result<()> {
    if let Some(webview) = app.get_webview(&slot.label()) {
        webview.set_bounds(rect_of(viewport))?;
    }
    Ok(())
}

/// What the label of the window an extension's toolbar popup opens in starts
/// with. Each popup gets a label of its own, since a window that is closing
/// keeps its label for a while; one is open at a time, as in other browsers.
const EXTENSION_POPUP: &str = "extension-popup-";

/// Source of toolbar popup labels, unique for the life of the process.
static NEXT_POPUP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Whether a window is an extension's toolbar popup.
pub fn is_extension_popup(label: &str) -> bool {
    label.starts_with(EXTENSION_POPUP)
}

/// The label of the toolbar popup open now. Locked while the popup is
/// measured, resized or shown, and while it is destroyed, so it is not
/// reached while it goes.
static OPEN_POPUP: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn open_popup() -> std::sync::MutexGuard<'static, Option<String>> {
    OPEN_POPUP.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// How often a toolbar popup's page is measured while it is open.
const POPUP_POLL: Duration = Duration::from_millis(100);

/// Longest a toolbar popup stays hidden waiting for its page to load. One
/// that never finishes is shown as it is.
const POPUP_SHOW_LIMIT: Duration = Duration::from_secs(3);

/// Opens an extension's toolbar popup on `url`, in a borderless window of
/// its own owned by `owner`, closing any other first.
///
/// The window starts hidden and at its smallest, so its page lays out at its
/// narrowest, as in Chrome; [`fit_extension_popup`] then sizes, places and
/// shows it. What the page asks to open or close is reported to `sink`.
///
/// @returns The popup window's label, for [`fit_extension_popup`].
///
/// # Errors
/// Returns [`HakuError::WindowMissing`] when `owner` is not open, and
/// propagates Tauri's failure to create the window and the platform's to
/// observe it.
pub fn open_extension_popup<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    owner: WindowId,
    url: &str,
    view: ExtensionView,
    sink: PageSink,
) -> Result<String> {
    // Held until the new popup is recorded, so two opens at once, as a
    // double click makes, cannot each leave a popup behind.
    let mut open = open_popup();
    destroy_popups(app, &mut open)?;
    let owner = native_window(app, owner)?;
    let label = format!(
        "{EXTENSION_POPUP}{}",
        NEXT_POPUP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let builder = tauri::WebviewWindowBuilder::new(app, &label, WebviewUrl::External(parse_url(url)?))
        .title("Haku")
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .inner_size(POPUP_MIN_SIZE.0, POPUP_MIN_SIZE.1)
        .user_agent(USER_AGENT)
        // Every webview sharing a profile must agree on these; see
        // `build_window`.
        .browser_extensions_enabled(true)
        .general_autofill_enabled(false)
        .initialization_script(inject::extension_page_script(view));
    // Owned, so it stays above its browser window and goes with it.
    #[cfg(windows)]
    let builder = builder.owner_raw(owner.hwnd()?);
    #[cfg(not(windows))]
    let _ = owner;
    let window = builder.build()?;
    *open = Some(label.clone());
    platform::observe_popup(window.as_ref(), sink)?;
    Ok(label)
}

/// Closes the extension's toolbar popup, if one is open.
///
/// Waits for [`fit_extension_popup`] to be done with it, so it must not be
/// called on the UI thread, which that waits on.
///
/// # Errors
/// Propagates Tauri's failure to destroy the window.
pub fn close_extension_popup<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<()> {
    destroy_popups(app, &mut open_popup())
}

fn destroy_popups<R: tauri::Runtime>(app: &tauri::AppHandle<R>, open: &mut Option<String>) -> Result<()> {
    *open = None;
    for (label, window) in app.webview_windows() {
        if is_extension_popup(&label) {
            window.destroy()?;
        }
    }
    Ok(())
}

/// Sizes the toolbar popup `label` to its page, places it below `anchor` inside
/// `area`, and shows it once its page has loaded, then keeps it fitting the
/// page as the page grows. Returns once the popup is closed.
///
/// A page that shrinks leaves the popup at the size it reached: it lays out
/// at the popup's width, so it never reports needing less.
///
/// Blocks, so it must not be called on the UI thread.
pub fn fit_extension_popup<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    label: &str,
    anchor: ScreenRect,
    area: ScreenRect,
    keep_open: impl Fn() -> bool,
) {
    let started = std::time::Instant::now();
    let mut size = POPUP_MIN_SIZE;
    let mut shown = false;
    loop {
        std::thread::sleep(POPUP_POLL);
        let open = open_popup();
        if open.as_deref() != Some(label) {
            return;
        }
        if !keep_open() {
            return;
        }
        let Some(window) = app.get_webview_window(label) else {
            return;
        };
        let measured = platform::evaluate(window.as_ref(), inject::popup_size_expression(), POPUP_POLL)
            .and_then(|json| inject::parse_popup_size(&json));
        // A window built hidden lays its page out at a size of the engine's
        // own, not the one it was built at, until it is resized. Only a page
        // laid out within the popup's size is measured.
        let fitting = measured.filter(|measured| measured.viewport.0 <= size.0 && measured.viewport.1 <= size.1);
        let loaded = fitting.is_some_and(|measured| measured.loaded);
        // Only ever grown: a page laid out wider than it needs, as one with
        // a scrollbar is, would otherwise shrink and grow back in turn.
        let wanted = fitting.map_or(size, |measured| {
            let (width, height) = popup_size(measured.size.0, measured.size.1);
            (width.max(size.0), height.max(size.1))
        });
        if wanted != size || !shown || (measured.is_some() && fitting.is_none()) {
            let (x, y) = popup_origin(anchor, wanted, area);
            let _ = window.set_size(LogicalSize::new(wanted.0, wanted.1));
            let _ = window.set_position(LogicalPosition::new(x, y));
            size = wanted;
        }
        if !shown && (loaded || started.elapsed() > POPUP_SHOW_LIMIT) {
            let _ = window.show();
            // Its page too, not only the window: keys go to the popup as in
            // other browsers, and only the page reports losing focus, which
            // is what closes it.
            let _ = window.set_focus();
            let _ = window.as_ref().set_focus();
            shown = true;
        }
        drop(open);
    }
}

fn parse_url(url: &str) -> Result<tauri::Url> {
    tauri::Url::parse(url).map_err(|error| HakuError::InvalidUrl(format!("{url}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_effect_that_fails_does_not_stop_the_ones_after_it() {
        let mut applied = Vec::new();
        let effects = [
            Effect::Reload { slot: SlotId(1) },
            Effect::Reload { slot: SlotId(2) },
            Effect::Show { slot: SlotId(3) },
        ];
        let (failed_slots, result) = each_effect(&effects, |effect| {
            applied.push(effect.clone());
            match effect {
                Effect::Reload { slot: SlotId(1) } => Err(HakuError::InvalidUrl("first".into())),
                Effect::Reload { .. } => Err(HakuError::InvalidUrl("second".into())),
                _ => Ok(()),
            }
        });
        assert_eq!(applied, effects);
        assert!(failed_slots.is_empty());
        assert!(matches!(result, Err(HakuError::InvalidUrl(url)) if url == "first"));
    }

    #[test]
    fn failed_native_creation_and_adoption_are_reported_for_model_repair() {
        let effects = [
            Effect::EnsureSlot {
                slot: SlotId(1),
                url: "https://create.test".into(),
                window: WindowId(1),
            },
            Effect::Adopt {
                slot: SlotId(2),
                request: WindowRequestId(1),
                url: "https://adopt.test".into(),
                window: WindowId(1),
            },
            Effect::Show { slot: SlotId(3) },
        ];
        let mut shown = false;
        let (failed_slots, result) = each_effect(&effects, |effect| match effect {
            Effect::Show { .. } => {
                shown = true;
                Ok(())
            }
            _ => Err(HakuError::Unsupported("native creation failed".into())),
        });
        assert!(shown, "the healthy sibling still gets its effects");
        assert!(result.is_err());
        assert_eq!(failed_slots, [SlotId(1), SlotId(2)]);
    }

    fn waiting() -> Option<Waiting> {
        WAITING_FOR_EXTENSIONS.lock().unwrap().clone()
    }

    // One test, because the waiting list is process-wide.
    #[test]
    fn extension_pages_wait_for_the_extensions_and_other_pages_do_not() {
        let page = "chrome-extension://abc/dashboard.html";

        assert!(hold_until_extensions(SlotId(1), Some(page), 1));
        assert!(!hold_until_extensions(SlotId(2), Some("https://a.test/"), 1));
        assert_eq!(waiting(), Some(HashMap::from([(SlotId(1), (page.to_owned(), 1))])));

        // Sent elsewhere, or destroyed, before the extensions are in.
        assert!(!hold_until_extensions(SlotId(1), Some(BLANK_URL), 2));
        assert!(hold_until_extensions(SlotId(3), Some(page), 1));
        hold_until_extensions(SlotId(3), None, 2);
        assert_eq!(waiting(), Some(HashMap::new()));

        // Once installed, nothing waits.
        WAITING_FOR_EXTENSIONS.lock().unwrap().take();
        assert!(!hold_until_extensions(SlotId(4), Some(page), 1));
        assert_eq!(waiting(), None);
    }

    #[test]
    fn each_new_destination_counts_up_per_slot() {
        let slot = SlotId(90);
        let first = next_load(slot);

        assert_eq!(next_load(slot), first + 1);
        assert_eq!(loads(slot), first + 1);
        assert_eq!(loads(SlotId(91)), 0);
    }
}

#[cfg(test)]
#[path = "viewport_tests.rs"]
mod viewport_tests;
