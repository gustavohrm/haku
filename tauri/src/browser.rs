use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{HakuError, Result};
use crate::model::{
    is_internal, Acquired, Commit, DialogAnswer, DialogId, DialogKind, Loss, LossSignal, NavigationKind, PageDialog,
    PageState, Placement, Policy, Pressure, Scroll, Slot, SlotId, Tab, TabId, TabPresence, Visit, WebviewPool,
    WindowId, WindowKind, WindowRequestId,
};

/// URL a slot is parked on after its tab is discarded.
///
/// Navigating away frees the page while keeping the webview itself alive, which
/// is far cheaper than destroying and recreating one on every tab switch.
pub const BLANK_URL: &str = "about:blank";

/// A tab left less than this long ago is kept whatever it would lose, so
/// flipping back to the tab just left is instant. Not honoured under pressure.
pub const GRACE_MS: u64 = 60_000;

/// A side effect the runtime must apply to a real webview.
///
/// [`Browser`] never touches Tauri. It decides what should be true and returns
/// the difference, which keeps the whole tab and pool policy testable without a
/// window, a webview, or an event loop.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Create the slot's webview in `window` if it does not exist, otherwise
    /// navigate it. An existing webview is already where it belongs: a
    /// [`Effect::Move`] precedes this when it is not.
    EnsureSlot {
        slot: SlotId,
        url: String,
        window: WindowId,
    },
    /// Park the slot on [`BLANK_URL`] to release the page it was holding.
    Blank {
        slot: SlotId,
    },
    Destroy {
        slot: SlotId,
    },
    Show {
        slot: SlotId,
    },
    Hide {
        slot: SlotId,
    },
    Reload {
        slot: SlotId,
    },
    /// Pause a hidden slot's page and let it give memory back.
    Freeze {
        slot: SlotId,
    },
    /// Undo [`Effect::Freeze`]. Precedes anything else done to a frozen slot.
    Resume {
        slot: SlotId,
    },
    /// Read the page before it is frozen, parked, destroyed or replaced by
    /// another tab's, and report what it holds with [`Browser::report_state`].
    /// Precedes the effect that leaves it.
    Leave {
        slot: SlotId,
        tab: TabId,
        /// Also capture what the page shows, as the preview its tab is
        /// covered with while it reloads. Set only while the page is still
        /// visible, which is the only time it can be captured.
        capture: bool,
    },
    /// Put a reloading tab's scroll and draft back once its document has
    /// loaded. Follows the [`Effect::EnsureSlot`] that reloads it.
    RestoreState {
        slot: SlotId,
        /// The URL they were read on. A page that loads on another origin
        /// does not receive them.
        url: String,
        scroll: Scroll,
        /// The document's height when the scroll was read, which the page
        /// waits to grow back to before its scroll counts as restored.
        height: Option<f64>,
        draft: Option<String>,
    },
    /// Release a page paused on a dialog, with the given answer.
    AnswerDialog {
        id: DialogId,
        answer: DialogAnswer,
    },
    /// Create the slot's webview, which does not exist yet, and hand it to the
    /// page that asked for a new window instead of navigating it, so the two
    /// stay connected through `window.opener`. Navigate it to `url` if the
    /// request can no longer take it.
    Adopt {
        slot: SlotId,
        request: WindowRequestId,
        url: String,
        window: WindowId,
    },
    /// Move the slot's webview into another window, for a tab there. Precedes
    /// anything else done to the slot for that tab.
    Move {
        slot: SlotId,
        window: WindowId,
    },
    /// Open a native window, with its chrome. Precedes every effect that puts
    /// a webview in it.
    OpenWindow {
        window: WindowId,
        kind: WindowKind,
        placement: Placement,
    },
    /// Close a native window. Every webview that was in it has been destroyed
    /// or moved out by the effects before this.
    CloseWindow {
        window: WindowId,
    },
    /// Bring a window to the front, when something the user did in another
    /// one opened a tab in it.
    FocusWindow {
        window: WindowId,
    },
}

/// How a page's request for a new window is opened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Opening {
    /// As a tab that becomes the active one.
    Foreground,
    /// As a tab left for later, which loads when it is selected.
    Background,
    /// As the active tab, in a new webview handed back to the request, so the
    /// page that opened it can still reach it.
    Connected(WindowRequestId),
    /// As the only tab of a new browser window.
    Window,
    /// In a popup window of its own, connected to the page as
    /// [`Opening::Connected`] is.
    Popup {
        request: WindowRequestId,
        placement: Placement,
    },
}

/// How many closed tabs are remembered for reopening.
pub const CLOSED_LIMIT: usize = 25;

/// A tab chosen by its place in the tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// The tab after the active one, wrapping to the first.
    Next,
    /// The tab before the active one, wrapping to the last.
    Previous,
    /// The tab at this position, counting from zero.
    Nth(usize),
    Last,
}

/// Which way a back or forward request moves through a tab's history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Back,
    Forward,
}

/// What a page report changed, for browsing history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageReport {
    /// The URL and title the tab now shows.
    pub url: String,
    pub title: String,
    /// Whether the page arrived somewhere: a followed link, a pushed route, or
    /// a navigation Haku made. A discarded tab reloading what it already
    /// showed is not a visit, and neither is a redirect, a replaced route or a
    /// retitle; those only refine the visit already recorded.
    pub visited: bool,
}

/// Why a tab holding a slot holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Position {
    Visible,
    /// Fixed, playing audio or capturing.
    MustRun,
    /// Kept in the background for what discarding it would lose, or for now.
    Kept,
}

/// The projection of browser state one window's interface renders.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub kind: WindowKind,
    /// The window's tabs, in order.
    pub tabs: Vec<Tab>,
    pub active: Option<TabId>,
    /// Exported as a JavaScript number: every value here is a counter, a
    /// small capacity, or a millisecond timestamp, all far inside the range a
    /// double represents exactly.
    #[specta(type = specta_typescript::Number)]
    pub capacity: usize,
    /// Tabs holding a webview, across every window: the pool is shared.
    #[specta(type = specta_typescript::Number)]
    pub live_count: usize,
}

/// One browser window, and the tab it shows.
struct Window {
    id: WindowId,
    kind: WindowKind,
    active: Option<TabId>,
}

/// A window's tabs as they are remembered, for [`Browser::restored`]: each
/// tab's URL and whether it is fixed, and the index of the active one.
pub type RestoredWindow = (Vec<(String, bool)>, Option<usize>);

pub struct Browser {
    /// Every window's tabs, in one list. A window's tabs are the ones it
    /// holds, in the order they appear here.
    tabs: Vec<Tab>,
    /// The window each tab belongs to.
    homes: HashMap<TabId, WindowId>,
    /// Open windows, the one used most recently last. There is always a
    /// normal one: closing the last is quitting, which is the caller's to do.
    windows: Vec<Window>,
    next_window: u64,
    pool: WebviewPool,
    /// The window each slot's webview is in. A webview stays where it is when
    /// it is parked, and moves when a tab in another window takes it.
    slot_windows: HashMap<SlotId, WindowId>,
    /// What each slot most recently loaded, so a redundant navigation is not
    /// emitted when a tab returns to a slot it never left.
    slot_urls: HashMap<SlotId, String>,
    /// Slots Haku has just navigated. The next URL such a slot commits is that
    /// navigation arriving, possibly redirected, and belongs to the entry that
    /// is already current rather than a new one.
    awaiting: HashSet<SlotId>,
    /// Tabs sent somewhere new whose page has not arrived yet. Their next
    /// awaited commit is a visit; any other awaited commit is a discarded tab
    /// coming back, which is not.
    navigated: HashSet<TabId>,
    next_id: u64,
    freeze: Policy,
    discard: Policy,
    /// The most memory kept tabs may hold together, in bytes.
    budget: u64,
    /// Hosts whose tabs are treated as holding work.
    kept_sites: Vec<String>,
    pressure: Pressure,
    /// Each slot's last measured memory, in bytes. A slot missing here is
    /// unmeasured and counts as holding nothing.
    memory: HashMap<SlotId, u64>,
    /// The latest time any caller has supplied, for mutations that supply
    /// none. Grace is judged against it.
    clock: u64,
    /// The slots last shown and the tab each was shown for. When such a tab
    /// stops being visible, its page is read and captured before anything
    /// else happens to it.
    shown: Vec<(SlotId, TabId)>,
    /// The tabs read by the current reconciliation as they stopped being
    /// visible, so they are not read a second time in the same pass.
    just_left: HashSet<TabId>,
    /// Recently closed tabs, the most recent last, with the window and place
    /// each stood in. Kept in memory only: they are not part of the session.
    closed: Vec<(WindowId, usize, Tab)>,
    /// The tab each page-opened tab was opened from, while both are open.
    openers: HashMap<TabId, TabId>,
    /// Connected tabs waiting, within the mutation that opened them, for a
    /// webview to hand to their request.
    adopting: HashMap<TabId, WindowRequestId>,
    /// Open tabs connected to the page that opened them. Their openers keep
    /// running while they are open: a sign-in popup reports back to the page
    /// that opened it. A tab that loses its page loses the connection with
    /// it, for good.
    connected: HashSet<TabId>,
}

impl Browser {
    /// A browser with one empty normal window.
    pub fn new(capacity: usize) -> Self {
        Self {
            tabs: Vec::new(),
            homes: HashMap::new(),
            windows: vec![Window {
                id: WindowId(1),
                kind: WindowKind::Normal,
                active: None,
            }],
            next_window: 2,
            pool: WebviewPool::new(capacity),
            slot_windows: HashMap::new(),
            slot_urls: HashMap::new(),
            awaiting: HashSet::new(),
            navigated: HashSet::new(),
            next_id: 1,
            // Nothing is frozen or discarded early until settings say so.
            freeze: Policy::Never,
            discard: Policy::Never,
            budget: 0,
            kept_sites: Vec::new(),
            pressure: Pressure::Normal,
            memory: HashMap::new(),
            clock: 0,
            shown: Vec::new(),
            just_left: HashSet::new(),
            closed: Vec::new(),
            openers: HashMap::new(),
            adopting: HashMap::new(),
            connected: HashSet::new(),
        }
    }

    /// Sets the optimization policies without touching any webview, for a
    /// browser that has none yet.
    pub fn with_policies(mut self, freeze: Policy, discard: Policy) -> Self {
        self.freeze = freeze;
        self.discard = discard;
        self
    }

    /// Sets what smart discarding keeps, without touching any webview.
    ///
    /// @param budget - The most memory kept tabs may hold together, in bytes.
    /// @param kept_sites - Hosts whose tabs are treated as holding work.
    pub fn with_keeping(mut self, budget: u64, kept_sites: Vec<String>) -> Self {
        self.set_keeping(budget, kept_sites);
        self
    }

    /// Rebuilds a browser from persisted tabs, touching no webviews.
    ///
    /// Restoring must not go through the ordinary open and select path: that
    /// path assumes its effects will be applied to real webviews, and at startup
    /// there are none and nowhere to put them. A browser built here has every
    /// tab discarded, which is exactly what reconciling against the first
    /// reported layout then acts on.
    ///
    /// The windows are given in the order they were last used, the most
    /// recent last, and are numbered from 1 in that order. A window with no
    /// tabs is skipped, and a browser restored with none has one empty window.
    pub fn restored(capacity: usize, windows: impl IntoIterator<Item = RestoredWindow>) -> Self {
        let mut browser = Self::new(capacity);
        browser.windows.clear();
        browser.next_window = 1;

        for (tabs, active) in windows {
            if tabs.is_empty() {
                continue;
            }
            let window = browser.add_window(WindowKind::Normal);
            let first = browser.tabs.len();
            for (url, fixed) in tabs {
                let id = TabId(browser.next_id);
                browser.next_id += 1;
                let mut tab = Tab::new(id, url);
                tab.fixed = fixed;
                browser.tabs.push(tab);
                browser.homes.insert(id, window);
            }
            let restored = &browser.tabs[first..];
            let active = active
                .and_then(|index| restored.get(index))
                .or_else(|| restored.first())
                .map(|tab| tab.id);
            if let Some(entry) = browser.window_mut(window) {
                entry.active = active;
            }
        }
        if browser.windows.is_empty() {
            browser.add_window(WindowKind::Normal);
        }
        browser
    }

    /// What the normal window used last shows.
    pub fn state(&self) -> BrowserState {
        self.state_in(self.main_window())
            .expect("the browser always has a normal window")
    }

    /// What one window shows, or nothing when no window has this id.
    pub fn state_in(&self, window: WindowId) -> Option<BrowserState> {
        let entry = self.window(window)?;
        Some(BrowserState {
            kind: entry.kind,
            tabs: self.tabs_in(window).cloned().collect(),
            active: entry.active,
            capacity: self.pool.capacity(),
            live_count: self.tabs.iter().filter(|tab| tab.slot().is_some()).count(),
        })
    }

    /// Every window's tabs.
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// The active tab of the normal window used last.
    pub fn active(&self) -> Option<TabId> {
        self.active_in(self.main_window())
    }

    pub fn active_in(&self, window: WindowId) -> Option<TabId> {
        self.window(window)?.active
    }

    /// Open windows, the one used most recently last.
    pub fn window_ids(&self) -> Vec<WindowId> {
        self.windows.iter().map(|window| window.id).collect()
    }

    pub fn kind(&self, window: WindowId) -> Option<WindowKind> {
        self.window(window).map(|window| window.kind)
    }

    /// The window a tab belongs to.
    pub fn window_of(&self, tab: TabId) -> Option<WindowId> {
        self.homes.get(&tab).copied()
    }

    /// The normal window used last: where a tab opened from nowhere in
    /// particular goes, and where a popup's tabs open.
    pub fn main_window(&self) -> WindowId {
        self.windows
            .iter()
            .rev()
            .find(|window| window.kind == WindowKind::Normal)
            .map(|window| window.id)
            .expect("the browser always has a normal window")
    }

    /// Whether closing this window would leave no normal window, which is
    /// quitting rather than closing.
    pub fn is_last_window(&self, window: WindowId) -> bool {
        self.kind(window) == Some(WindowKind::Normal)
            && self
                .windows
                .iter()
                .filter(|entry| entry.kind == WindowKind::Normal)
                .count()
                == 1
    }

    /// The slots whose webviews are in a window.
    pub fn slots_in(&self, window: WindowId) -> Vec<SlotId> {
        self.pool
            .slots()
            .iter()
            .map(|slot| slot.id)
            .filter(|slot| self.slot_windows.get(slot) == Some(&window))
            .collect()
    }

    fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|window| window.id == id)
    }

    fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find(|window| window.id == id)
    }

    /// Adds a window, as the one used most recently.
    fn add_window(&mut self, kind: WindowKind) -> WindowId {
        let id = WindowId(self.next_window);
        self.next_window += 1;
        self.windows.push(Window { id, kind, active: None });
        id
    }

    fn tabs_in(&self, window: WindowId) -> impl Iterator<Item = &Tab> {
        self.tabs
            .iter()
            .filter(move |tab| self.homes.get(&tab.id) == Some(&window))
    }

    /// The tab each window shows.
    fn visible(&self) -> Vec<TabId> {
        self.windows.iter().filter_map(|window| window.active).collect()
    }

    fn is_visible(&self, id: TabId) -> bool {
        self.windows.iter().any(|window| window.active == Some(id))
    }

    /// The window tabs opened from `window` go in: itself, unless it is a
    /// popup, which never takes another tab.
    fn tab_window(&self, window: WindowId) -> WindowId {
        match self.kind(window) {
            Some(WindowKind::Normal) => window,
            _ => self.main_window(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.pool.capacity()
    }

    /// Every slot the pool currently owns, with its occupant if it has one.
    pub fn slots(&self) -> &[Slot] {
        self.pool.slots()
    }

    /// Every slot the pool currently owns, whether occupied or parked.
    pub fn slot_ids(&self) -> Vec<SlotId> {
        self.pool.slots().iter().map(|slot| slot.id).collect()
    }

    fn index_of(&self, id: TabId) -> Result<usize> {
        self.tabs
            .iter()
            .position(|tab| tab.id == id)
            .ok_or_else(|| HakuError::TabNotFound(id.0.to_string()))
    }

    fn tab_mut(&mut self, id: TabId) -> Result<&mut Tab> {
        let index = self.index_of(id)?;
        Ok(&mut self.tabs[index])
    }

    pub fn tab(&self, id: TabId) -> Result<&Tab> {
        let index = self.index_of(id)?;
        Ok(&self.tabs[index])
    }

    pub fn budget(&self) -> u64 {
        self.budget
    }

    /// The reasons discarding a tab would cost something, kept sites included.
    pub fn loss_signals(&self, id: TabId) -> Vec<LossSignal> {
        self.tab(id)
            .map(|tab| tab.loss_signals(&self.kept_sites))
            .unwrap_or_default()
    }

    fn loss(&self, tab: &Tab) -> Loss {
        tab.loss(&self.kept_sites)
    }

    /// Why a tab holds a slot, or nothing when it holds none.
    pub fn position(&self, id: TabId) -> Option<Position> {
        let tab = self.tab(id).ok()?;
        tab.slot()?;
        Some(if self.is_visible(id) {
            Position::Visible
        } else if self.must_run(tab) {
            Position::MustRun
        } else {
            Position::Kept
        })
    }

    /// Advances the clock to `now`, which never runs backwards.
    fn advance(&mut self, now: u64) {
        self.clock = self.clock.max(now);
    }

    /// Makes a tab the one its window shows, timing the tab it replaces from
    /// now: a tab is in grace from when it was left.
    fn activate(&mut self, id: TabId) {
        let clock = self.clock;
        let Some(window) = self.window_of(id) else { return };
        if let Some(previous) = self.active_in(window).filter(|&previous| previous != id) {
            if let Ok(tab) = self.tab_mut(previous) {
                tab.active_at = tab.active_at.max(clock);
            }
        }
        if let Some(window) = self.window_mut(window) {
            window.active = Some(id);
        }
    }

    /// Records that a window is the one in use, which is where tabs opened
    /// from nowhere in particular go.
    pub fn focus(&mut self, window: WindowId) {
        if let Some(index) = self.windows.iter().position(|entry| entry.id == window) {
            let entry = self.windows.remove(index);
            self.windows.push(entry);
        }
    }

    /// Drops what the browser remembered about a slot it has let go of.
    fn forget_slot(&mut self, slot: SlotId) {
        self.slot_urls.remove(&slot);
        self.awaiting.remove(&slot);
    }

    /// Forgets a slot whose webview is being destroyed.
    fn drop_slot(&mut self, slot: SlotId) {
        self.forget_slot(slot);
        self.slot_windows.remove(&slot);
    }

    /// Releases a tab's pending dialog because its page is going away.
    fn dismiss_dialog(&mut self, id: TabId) -> Option<Effect> {
        let dialog = self.tab_mut(id).ok()?.dialog.take()?;
        Some(Effect::AnswerDialog {
            id: dialog.id,
            answer: dialog.abandoned(),
        })
    }

    /// Whether a tab must keep running in the background: it is fixed, a tab
    /// connected to it is still open, or its page is playing audio or
    /// capturing the camera, microphone or screen.
    ///
    /// Audio and capture are not honoured under an _always_ policy. Always
    /// means always, and fixing the tab is how a user exempts it. A connected
    /// tab is: discarding its opener breaks the sign-in it is for.
    fn must_run(&self, tab: &Tab) -> bool {
        let honours_signals = self.freeze != Policy::Always && self.discard != Policy::Always;
        let signalled = tab.audible || tab.page.capturing();
        tab.fixed
            || (tab.slot().is_some() && self.has_connected(tab.id))
            || (honours_signals && signalled && tab.slot().is_some())
    }

    fn has_connected(&self, id: TabId) -> bool {
        self.connected.iter().any(|tab| self.openers.get(tab) == Some(&id))
    }

    /// Slots that must exist whatever the configured capacity: one for each
    /// window's visible tab, one per fixed tab, and one per background tab
    /// that must run for another reason.
    fn reserved(&self) -> usize {
        let visible = self.visible();
        let fixed = self.tabs.iter().filter(|tab| tab.fixed && !tab.is_internal()).count();
        let running = self
            .tabs
            .iter()
            .filter(|tab| !tab.fixed && !visible.contains(&tab.id) && self.must_run(tab))
            .count();
        visible.len().max(1) + fixed + running
    }

    fn effective_capacity(&self) -> usize {
        self.pool.effective_capacity(self.reserved())
    }

    /// Whether a tab must keep its slot: it is visible, or it must run.
    fn is_protected(&self, id: TabId) -> bool {
        self.is_visible(id) || self.tab(id).is_ok_and(|tab| self.must_run(tab))
    }

    /// The tab to give up its slot when the pool is full: the unprotected
    /// occupant that would lose least, and among equals the one shown least
    /// recently. A tab holding work is taken only when nothing else can be.
    fn victim(&self) -> Option<TabId> {
        self.pool
            .slots()
            .iter()
            .filter_map(|slot| slot.occupant.map(|occupant| (occupant, slot.used_at)))
            .filter(|&(occupant, _)| !self.is_protected(occupant))
            .min_by_key(|&(occupant, used_at)| {
                let loss = self.tab(occupant).map(|tab| self.loss(tab)).unwrap_or_default();
                (loss, used_at)
            })
            .map(|(occupant, _)| occupant)
    }

    /// The slot to destroy when the pool shrinks: a parked one if there is
    /// one, otherwise the victim's.
    fn surplus_slot(&self) -> Option<SlotId> {
        let slots = self.pool.slots();
        slots
            .iter()
            .find(|slot| slot.occupant.is_none())
            .or_else(|| {
                let victim = self.victim()?;
                slots.iter().find(|slot| slot.occupant == Some(victim))
            })
            .map(|slot| slot.id)
    }

    // -- mutations -------------------------------------------------------

    /// Opens a tab in the normal window used last.
    pub fn open_tab(&mut self, url: impl Into<String>, activate: bool) -> (TabId, Vec<Effect>) {
        self.open_tab_in(self.main_window(), url, activate)
    }

    /// Opens a tab in `window`, or in the normal window used last when
    /// `window` is a popup, which is then brought to the front.
    pub fn open_tab_in(&mut self, window: WindowId, url: impl Into<String>, activate: bool) -> (TabId, Vec<Effect>) {
        let home = self.tab_window(window);
        let mut effects = self.focus_elsewhere(window, home);
        let id = self.add_tab(self.tabs.len(), home, url);

        if activate || self.active_in(home).is_none() {
            self.activate(id);
        }
        effects.extend(self.realize());
        (id, effects)
    }

    /// Opens a new browser window on `url`, as the window in use.
    ///
    /// @returns The window's one tab.
    pub fn open_window(&mut self, url: impl Into<String>) -> (TabId, Vec<Effect>) {
        let window = self.add_window(WindowKind::Normal);
        let id = self.add_tab(self.tabs.len(), window, url);
        self.activate(id);
        let mut effects = vec![Effect::OpenWindow {
            window,
            kind: WindowKind::Normal,
            placement: Placement::default(),
        }];
        effects.extend(self.realize());
        (id, effects)
    }

    fn add_tab(&mut self, index: usize, window: WindowId, url: impl Into<String>) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        self.tabs.insert(index, Tab::new(id, url));
        self.homes.insert(id, window);
        self.navigated.insert(id);
        id
    }

    /// Brings `to` to the front when what the user did in `from` opened
    /// something there instead.
    fn focus_elsewhere(&mut self, from: WindowId, to: WindowId) -> Vec<Effect> {
        if from == to {
            return Vec::new();
        }
        self.focus(to);
        vec![Effect::FocusWindow { window: to }]
    }

    /// Opens the window a page in `opener` asked for, as `opening` says.
    ///
    /// A tab opens next to the page's own, after any it opened before, in the
    /// page's window; a page in a popup opens its tabs in the normal window
    /// used last. A window opens with the request's page as its only tab.
    ///
    /// A connected tab gets a webview that has never loaded anything, as the
    /// engine requires of one it hands to the page. When every slot is spoken
    /// for, it gets none: no [`Effect::Adopt`] is returned, the request goes
    /// unanswered by a webview, and the tab loads `url` like any other once
    /// it gets a slot.
    pub fn open_from(&mut self, opener: SlotId, url: impl Into<String>, opening: Opening) -> (TabId, Vec<Effect>) {
        let parent = self.occupant_of(opener);
        let source = parent
            .and_then(|parent| self.window_of(parent))
            .unwrap_or_else(|| self.main_window());

        let mut effects = Vec::new();
        let (id, request) = match opening {
            Opening::Window | Opening::Popup { .. } => {
                let (kind, placement, request) = match opening {
                    Opening::Popup { request, placement } => (WindowKind::Popup, placement, Some(request)),
                    _ => (WindowKind::Normal, Placement::default(), None),
                };
                let window = self.add_window(kind);
                effects.push(Effect::OpenWindow {
                    window,
                    kind,
                    placement,
                });
                (self.add_tab(self.tabs.len(), window, url), request)
            }
            Opening::Foreground | Opening::Background | Opening::Connected(_) => {
                let home = self.tab_window(source);
                effects.extend(self.focus_elsewhere(source, home));
                let index = self.index_after(parent.filter(|&parent| self.window_of(parent) == Some(home)));
                let id = self.add_tab(index, home, url);
                let request = match opening {
                    Opening::Connected(request) => Some(request),
                    _ => None,
                };
                (id, request)
            }
        };
        if let Some(parent) = parent.filter(|_| opening != Opening::Window) {
            self.openers.insert(id, parent);
        }

        let home = self.window_of(id).unwrap_or(source);
        match opening {
            Opening::Background if self.active_in(home).is_some() => {}
            _ => self.activate(id),
        }
        if let Some(request) = request {
            self.adopting.insert(id, request);
            self.connected.insert(id);
        }
        effects.extend(self.realize());
        self.adopting.remove(&id);
        (id, effects)
    }

    /// Where a tab opened from `parent` goes in the list: after it, and after
    /// the tabs it opened before. At the end without one.
    fn index_after(&self, parent: Option<TabId>) -> usize {
        let Some(at) = parent.and_then(|parent| self.index_of(parent).ok()) else {
            return self.tabs.len();
        };
        let mut index = at + 1;
        while self
            .tabs
            .get(index)
            .is_some_and(|tab| self.openers.get(&tab.id) == parent.as_ref())
        {
            index += 1;
        }
        index
    }

    /// The tab whose page is in `slot`, if any.
    pub fn tab_in(&self, slot: SlotId) -> Option<TabId> {
        self.occupant_of(slot)
    }

    /// Closes a tab, moving activation to its right-hand neighbour, or back to
    /// the tab that opened it.
    ///
    /// A normal window always keeps a tab: closing its last one opens
    /// `replacement` in its place, the page a new tab would show. A popup's
    /// one tab closes its window.
    ///
    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn close_tab(&mut self, id: TabId, replacement: &str) -> Result<Vec<Effect>> {
        let index = self.index_of(id)?;
        let window = self.window_of(id).unwrap_or_else(|| self.main_window());
        if self.kind(window) == Some(WindowKind::Popup) {
            return Ok(self.close_window(window));
        }
        let place = self.tabs_in(window).position(|tab| tab.id == id).unwrap_or_default();
        let mut effects: Vec<Effect> = self.dismiss_dialog(id).into_iter().collect();
        effects.extend(self.release_slot(id));
        let closed = self.tabs.remove(index);
        self.homes.remove(&id);
        self.navigated.remove(&id);
        self.remember_closed(window, index, closed);
        // A page-opened tab, such as a sign-in popup, closing returns to the
        // page that opened it, when that page is in the same window.
        let opener = self
            .openers
            .remove(&id)
            .filter(|opener| self.window_of(*opener) == Some(window));
        self.openers.retain(|_, parent| *parent != id);
        self.connected.remove(&id);

        let remaining: Vec<TabId> = self.tabs_in(window).map(|tab| tab.id).collect();
        if remaining.is_empty() {
            if let Some(entry) = self.window_mut(window) {
                entry.active = None;
            }
            let (_, opened) = self.open_tab_in(window, replacement, true);
            effects.extend(opened);
            return Ok(effects);
        }

        if self.active_in(window) == Some(id) {
            let next = opener.or_else(|| remaining.get(place).or_else(|| remaining.last()).copied());
            if let Some(entry) = self.window_mut(window) {
                entry.active = next;
            }
        }

        effects.extend(self.realize());
        Ok(effects)
    }

    /// Closes a window and every tab in it, destroying the webviews in it.
    ///
    /// Closing the only normal window does nothing: that is quitting, which
    /// keeps the window's tabs for the next launch, and is the caller's to do.
    /// The tabs of a closed window are not remembered for reopening.
    pub fn close_window(&mut self, window: WindowId) -> Vec<Effect> {
        if self.window(window).is_none() || self.is_last_window(window) {
            return Vec::new();
        }
        let closing: HashSet<TabId> = self.tabs_in(window).map(|tab| tab.id).collect();
        let mut effects: Vec<Effect> = closing.iter().filter_map(|&id| self.dismiss_dialog(id)).collect();

        // A webview goes with its window, whichever tab holds it.
        for slot in self.slots_in(window) {
            let Some(removed) = self.pool.remove(slot) else {
                continue;
            };
            if let Some(occupant) = removed.occupant.filter(|occupant| !closing.contains(occupant)) {
                effects.extend(self.dismiss_dialog(occupant));
                effects.extend(self.leave(occupant));
                if let Ok(tab) = self.tab_mut(occupant) {
                    tab.lose_page();
                }
                self.connected.remove(&occupant);
            }
            self.drop_slot(slot);
            effects.push(Effect::Destroy { slot });
        }

        self.tabs.retain(|tab| !closing.contains(&tab.id));
        for id in &closing {
            self.homes.remove(id);
            self.navigated.remove(id);
            self.openers.remove(id);
            self.connected.remove(id);
            self.adopting.remove(id);
        }
        self.openers.retain(|_, parent| !closing.contains(parent));
        self.shown.retain(|(_, tab)| !closing.contains(tab));
        self.windows.retain(|entry| entry.id != window);

        effects.push(Effect::CloseWindow { window });
        effects.extend(self.realize());
        effects
    }

    /// Reopens the most recently closed tab where it stood, with its history,
    /// and makes it active. A tab whose window has closed since reopens in the
    /// normal window used last. Does nothing when no closed tab is remembered.
    pub fn reopen_closed_tab(&mut self) -> Vec<Effect> {
        let Some((window, index, mut tab)) = self.closed.pop() else {
            return Vec::new();
        };
        let main = self.main_window();
        let window = if self.window(window).is_some() { window } else { main };
        let mut effects = self.focus_elsewhere(main, window);
        tab.id = TabId(self.next_id);
        self.next_id += 1;
        let id = tab.id;
        self.tabs.insert(index.min(self.tabs.len()), tab);
        self.homes.insert(id, window);
        self.activate(id);
        effects.extend(self.realize());
        effects
    }

    /// Keeps a closed tab for reopening, as a discarded page that reloads into
    /// whichever slot it gets, with its scroll and draft.
    ///
    /// A tab that never left an internal page, such as a new tab opened and
    /// closed again, holds nothing worth reopening.
    fn remember_closed(&mut self, window: WindowId, index: usize, mut tab: Tab) {
        if tab.history.entries().len() == 1 && tab.is_internal() {
            return;
        }
        tab.lose_page();
        tab.reclassify();
        tab.dialog = None;
        tab.relieved = false;
        self.closed.push((window, index, tab));
        if self.closed.len() > CLOSED_LIMIT {
            self.closed.remove(0);
        }
    }

    /// The tab a pick lands on in the normal window used last, if there is one.
    pub fn pick(&self, pick: Pick) -> Option<TabId> {
        self.pick_in(self.main_window(), pick)
    }

    /// The tab a pick lands on in `window`, if there is one.
    pub fn pick_in(&self, window: WindowId, pick: Pick) -> Option<TabId> {
        let tabs: Vec<TabId> = self.tabs_in(window).map(|tab| tab.id).collect();
        let count = tabs.len();
        let active = self
            .active_in(window)
            .and_then(|id| tabs.iter().position(|&tab| tab == id));
        let index = match pick {
            Pick::Next => (active? + 1) % count,
            Pick::Previous => (active? + count - 1) % count,
            Pick::Nth(index) => index,
            Pick::Last => count.checked_sub(1)?,
        };
        tabs.get(index).copied()
    }

    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn select_tab(&mut self, id: TabId, now: u64) -> Result<Vec<Effect>> {
        self.index_of(id)?;
        self.advance(now);
        self.activate(id);
        self.pool.touch(id);
        // Viewing a tab is the activity signal available without giving remote
        // pages a channel back into the application.
        self.tab_mut(id)?.active_at = now;
        Ok(self.realize())
    }

    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn navigate(&mut self, id: TabId, url: impl Into<String>) -> Result<Vec<Effect>> {
        let dismissed = self.dismiss_dialog(id);
        let tab = self.tab_mut(id)?;
        tab.history.push(Visit::new(url));
        tab.leave_entry();
        tab.reclassify();
        self.navigated.insert(id);

        // The tab may have just left the pool for an internal page.
        if self.tab(id)?.is_internal() {
            let released = self.release_slot(id);
            if !released.is_empty() {
                let mut effects: Vec<Effect> = dismissed.into_iter().collect();
                effects.extend(released);
                effects.extend(self.realize());
                return Ok(effects);
            }
        }
        let mut effects: Vec<Effect> = dismissed.into_iter().collect();
        effects.extend(self.realize());
        Ok(effects)
    }

    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn go_back(&mut self, id: TabId) -> Result<Vec<Effect>> {
        self.step_history(id, true)
    }

    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn go_forward(&mut self, id: TabId) -> Result<Vec<Effect>> {
        self.step_history(id, false)
    }

    fn step_history(&mut self, id: TabId, backwards: bool) -> Result<Vec<Effect>> {
        let tab = self.tab(id)?;
        let can_move = if backwards {
            tab.history.can_go_back()
        } else {
            tab.history.can_go_forward()
        };
        if !can_move {
            return Ok(Vec::new());
        }

        let mut effects: Vec<Effect> = self.dismiss_dialog(id).into_iter().collect();
        let tab = self.tab_mut(id)?;
        if backwards {
            tab.history.go_back();
        } else {
            tab.history.go_forward();
        }
        tab.leave_entry();
        tab.reclassify();
        self.navigated.insert(id);
        effects.extend(self.realize());
        Ok(effects)
    }

    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn reload(&mut self, id: TabId) -> Result<Vec<Effect>> {
        let slot = self.tab(id)?.slot();
        let mut effects: Vec<Effect> = self.dismiss_dialog(id).into_iter().collect();
        match slot {
            Some(slot) => effects.push(Effect::Reload { slot }),
            None => effects.extend(self.realize()),
        }
        Ok(effects)
    }

    /// Pins or unpins a tab. A fixed tab keeps its webview until it is closed or
    /// unpinned; no policy discards it.
    ///
    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn set_fixed(&mut self, id: TabId, fixed: bool) -> Result<Vec<Effect>> {
        self.tab_mut(id)?.fixed = fixed;
        Ok(self.realize())
    }

    /// Moves a tab to position `to` among its window's tabs.
    ///
    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn reorder_tab(&mut self, id: TabId, to: usize) -> Result<()> {
        let from = self.index_of(id)?;
        let window = self.window_of(id);
        let tab = self.tabs.remove(from);
        let places: Vec<usize> = (0..self.tabs.len())
            .filter(|&index| self.homes.get(&self.tabs[index].id) == window.as_ref())
            .collect();
        let at = match places.get(to) {
            Some(&at) => at,
            None => places.last().map_or(self.tabs.len(), |&last| last + 1),
        };
        self.tabs.insert(at, tab);
        Ok(())
    }

    /// Applies the user's optimization settings.
    ///
    /// @param capacity - The pool size to use, which is 1 while every
    ///   background tab is discarded whatever the user configured.
    pub fn set_optimization(&mut self, capacity: usize, freeze: Policy, discard: Policy) -> Vec<Effect> {
        self.freeze = freeze;
        self.discard = discard;
        self.set_capacity(capacity)
    }

    /// Sets what smart discarding keeps. Takes effect on the next change or
    /// tick, so it is set before [`Browser::set_optimization`].
    ///
    /// @param budget - The most memory kept tabs may hold together, in bytes.
    /// @param kept_sites - Hosts whose tabs are treated as holding work.
    pub fn set_keeping(&mut self, budget: u64, kept_sites: Vec<String>) {
        self.budget = budget;
        self.kept_sites = kept_sites;
    }

    /// Records how short of memory the machine is, read as a tab is selected
    /// or opened: a burst of tab switches is when memory runs out faster than
    /// the tick notices.
    pub fn set_pressure(&mut self, pressure: Pressure) {
        self.pressure = pressure;
    }

    pub fn set_capacity(&mut self, capacity: usize) -> Vec<Effect> {
        self.pool.set_capacity(capacity);
        let mut effects = self.trim();
        effects.extend(self.realize());
        effects
    }

    /// Destroys webviews until the pool is back within its capacity.
    ///
    /// Parked webviews go first, then the tabs eviction would pick, so
    /// trimming never takes a visible or must-run tab's webview.
    fn trim(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        while self.pool.slots().len() > self.effective_capacity() {
            let Some(removed) = self.surplus_slot().and_then(|slot| self.pool.remove(slot)) else {
                break;
            };
            if let Some(occupant) = removed.occupant {
                effects.extend(self.dismiss_dialog(occupant));
                effects.extend(self.leave(occupant));
                if let Ok(tab) = self.tab_mut(occupant) {
                    tab.lose_page();
                }
                self.connected.remove(&occupant);
            }
            self.drop_slot(removed.id);
            effects.push(Effect::Destroy { slot: removed.id });
        }
        effects
    }

    /// Brings webviews back in line with tab state without changing anything.
    ///
    /// Needed once the interface reports where the viewport is: a restored
    /// session has tabs but no webviews, and there is nowhere to put one until
    /// the chrome has been laid out.
    pub fn reconcile(&mut self) -> Vec<Effect> {
        self.realize()
    }

    /// Takes the tick's reading and applies the rule to every background tab;
    /// see [`Browser::smart_discards`].
    ///
    /// This is also where the visible tab is timed: a tab is stamped when it
    /// is selected, when it is left, and on every tick while it stays in view.
    /// A tab never stamped is timed from the first tick that sees it.
    ///
    /// @param memory - Each slot's memory in bytes; a slot missing is unmeasured.
    /// @returns Nothing when no tab changed, which most ticks are.
    pub fn tick(&mut self, now: u64, pressure: Pressure, memory: HashMap<SlotId, u64>) -> Vec<Effect> {
        self.advance(now);
        let visible = self.visible();
        for tab in &mut self.tabs {
            if visible.contains(&tab.id) || tab.active_at == 0 {
                tab.active_at = now;
            }
        }
        self.pressure = pressure;
        self.memory = memory;
        self.settle_background()
    }

    // -- reports from the page -------------------------------------------

    /// Records whether a slot's page is playing audio.
    ///
    /// A smart policy leaves a playing tab running, so a tab that falls silent
    /// in the background may be frozen now.
    ///
    /// A background tab that falls silent becomes an ordinary background tab,
    /// timed from `now` rather than from when it was last shown.
    pub fn report_audio(&mut self, slot: SlotId, playing: bool, now: u64) -> Vec<Effect> {
        self.advance(now);
        let Some(id) = self.occupant_of(slot) else {
            return Vec::new();
        };
        match self.tab_mut(id) {
            Ok(tab) if tab.audible != playing => {
                tab.audible = playing;
                if !playing {
                    tab.active_at = now;
                }
            }
            _ => return Vec::new(),
        }
        self.realize()
    }

    /// Records a reading of a tab's page.
    ///
    /// Keyed by tab rather than slot: by the time a reading completes, the
    /// slot may belong to another tab. A tab that started or stopped capturing
    /// may have to keep running, or may now be frozen. A change in what the
    /// tab would lose is acted on at the next change or tick, not here.
    ///
    /// @param state - The reading, or nothing when the page could not be read.
    pub fn report_state(&mut self, id: TabId, state: Option<PageState>) -> Vec<Effect> {
        let Ok(tab) = self.tab_mut(id) else {
            return Vec::new();
        };
        let was_capturing = tab.page.capturing();
        tab.record_state(state);
        if tab.page.capturing() == was_capturing {
            return Vec::new();
        }
        self.realize()
    }

    /// Records that a navigation started in a slot, and whether it carries a
    /// form submission. A page loaded from one is the user's work.
    pub fn report_navigation(&mut self, slot: SlotId, form: bool) {
        let Some(id) = self.occupant_of(slot) else { return };
        if let Ok(tab) = self.tab_mut(id) {
            tab.page.form_result = form;
        }
    }

    /// Records that a slot's document has loaded, which ends its tab's
    /// restore. The blank page a slot is created or parked on is not that
    /// document.
    ///
    /// @returns Whether a tab stopped restoring, which the interface shows.
    pub fn report_loaded(&mut self, slot: SlotId, url: &str) -> bool {
        if url == BLANK_URL {
            return false;
        }
        let Some(id) = self.occupant_of(slot) else {
            return false;
        };
        match self.tab_mut(id) {
            Ok(tab) if tab.restoring => {
                tab.restoring = false;
                true
            }
            _ => false,
        }
    }

    /// The running pages worth reading on a tick: the visible tab's and every
    /// background tab's that is not frozen. A frozen page cannot change.
    pub fn running_pages(&self) -> Vec<(SlotId, TabId)> {
        self.tabs
            .iter()
            .filter_map(|tab| match tab.presence {
                TabPresence::Live { slot } => Some((slot, tab.id)),
                _ => None,
            })
            .collect()
    }

    fn occupant_of(&self, slot: SlotId) -> Option<TabId> {
        self.pool
            .slots()
            .iter()
            .find(|candidate| candidate.id == slot)
            .and_then(|found| found.occupant)
    }

    /// Records what a slot's webview is actually showing.
    ///
    /// `commits` are the URL changes the page made since the last report, in
    /// order, each classified by the page itself: a followed link or a pushed
    /// route adds an entry, a redirect or a replaced route rewrites the current
    /// one, and the page's own back or forward steps through existing entries.
    /// Without that classification a single-page app's route changes would be
    /// invisible, and returning to its tab would reload an older URL.
    ///
    /// `title` is the document's title after all of them, so it always lands
    /// on the entry the page ended up on.
    ///
    /// @returns What the tab now shows and whether that was a visit.
    pub fn report_page(&mut self, slot: SlotId, commits: &[Commit], title: Option<String>) -> Option<PageReport> {
        let id = self.occupant_of(slot)?;
        let commits: Vec<&Commit> = commits.iter().filter(|commit| commit.url != BLANK_URL).collect();
        if commits.is_empty() && title.is_none() {
            return None;
        }

        let mut visited = false;
        for commit in commits {
            self.slot_urls.insert(slot, commit.url.clone());
            let awaited = self.awaiting.remove(&slot);
            visited |= if awaited {
                self.navigated.remove(&id)
            } else {
                commit.kind == NavigationKind::Push
            };
            let favicon = favicon_for(&commit.url);
            let tab = self.tab_mut(id).ok()?;

            match commit.kind {
                // What Haku asked for has arrived. Whatever URL it settled on is
                // the entry already on the stack.
                _ if awaited => tab.history.update_current(Some(commit.url.clone()), None, favicon),
                NavigationKind::Push => {
                    // The document keeps its title across a pushed route until
                    // the page changes it, and so does the new entry.
                    let mut visit = Visit::new(commit.url.clone());
                    visit.title = tab.history.current().title.clone();
                    visit.favicon = favicon;
                    tab.history.push(visit);
                    tab.leave_entry();
                }
                NavigationKind::Traverse if tab.history.step_to(&commit.url) => {}
                NavigationKind::Traverse | NavigationKind::Replace | NavigationKind::Reload => {
                    tab.history.update_current(Some(commit.url.clone()), None, favicon);
                }
            }
        }

        let tab = self.tab_mut(id).ok()?;
        if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
            tab.history.update_current(None, Some(title), None);
        }

        let current = tab.history.current();
        Some(PageReport {
            url: current.url.clone(),
            title: current.title.clone(),
            visited,
        })
    }

    /// Decides what a webview's own back or forward should do.
    ///
    /// A webview's native history also holds pages of other tabs that used the
    /// same slot, so it cannot be followed. The request is answered from the
    /// tab's history instead: forward if it names the next entry, otherwise
    /// back, which is what a back button or gesture almost always means.
    ///
    /// @returns The tab to move and the direction, or nothing for an empty slot.
    pub fn traversal(&self, slot: SlotId, url: &str) -> Option<(TabId, Direction)> {
        let id = self.occupant_of(slot)?;
        let history = &self.tab(id).ok()?.history;
        let forward = history.can_go_forward() && history.entries()[history.index() + 1].url == url;
        Some((id, if forward { Direction::Forward } else { Direction::Back }))
    }

    /// Attaches a dialog a page opened to the tab that owns the page.
    ///
    /// A dialog nobody can see is answered at once instead: one from a slot no
    /// tab owns, or a "Leave site?" raised while Haku itself is navigating the
    /// slot, when switching tabs or going back. The owner of the slot may
    /// already be a different tab by then, and asking it would be wrong.
    pub fn open_dialog(&mut self, slot: SlotId, dialog: PageDialog) -> Vec<Effect> {
        let unattended = dialog.kind == DialogKind::BeforeUnload && self.awaiting.contains(&slot);
        let Some(id) = self.occupant_of(slot).filter(|_| !unattended) else {
            return vec![Effect::AnswerDialog {
                id: dialog.id,
                answer: dialog.abandoned(),
            }];
        };

        // A page is paused while its dialog is open, so it cannot open a second
        // one; a stale one left behind is released rather than stranded.
        let mut effects: Vec<Effect> = self.dismiss_dialog(id).into_iter().collect();
        if let Ok(tab) = self.tab_mut(id) {
            tab.dialog = Some(dialog);
        } else {
            effects.push(Effect::AnswerDialog {
                id: dialog.id,
                answer: dialog.abandoned(),
            });
        }
        effects
    }

    /// Answers the dialog a tab is showing.
    ///
    /// An answer to a dialog that is no longer open is ignored, so a click that
    /// races a page going away cannot reach a newer dialog.
    ///
    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn answer_dialog(&mut self, id: TabId, dialog: DialogId, answer: DialogAnswer) -> Result<Vec<Effect>> {
        let tab = self.tab_mut(id)?;
        if tab.dialog.as_ref().map(|open| open.id) != Some(dialog) {
            return Ok(Vec::new());
        }
        tab.dialog = None;
        Ok(vec![Effect::AnswerDialog { id: dialog, answer }])
    }

    // -- reconciliation ---------------------------------------------------

    /// Brings webviews in line with tab state and returns the difference.
    ///
    /// Every mutation ends here rather than emitting effects itself, so there is
    /// exactly one description of what "correct" looks like.
    fn realize(&mut self) -> Vec<Effect> {
        let visible = self.visible();
        // Before anything hides it or takes its slot: a page can only be
        // captured while it is still on screen.
        let mut effects = self.leave_shown();

        for &id in &visible {
            effects.extend(self.bind(id));
        }

        // Fixed tabs stay resident behind the visible ones, capacity permitting.
        let fixed: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|tab| tab.fixed && !tab.is_internal() && !visible.contains(&tab.id))
            .map(|tab| tab.id)
            .collect();
        for id in fixed {
            effects.extend(self.bind(id));
        }

        self.shown = visible
            .iter()
            .filter_map(|&id| Some((self.tab(id).ok()?.slot()?, id)))
            .collect();
        for slot in self.pool.slots().iter().map(|slot| slot.id).collect::<Vec<_>>() {
            if self.shown.iter().any(|&(shown, _)| shown == slot) {
                effects.push(Effect::Show { slot });
            } else {
                effects.push(Effect::Hide { slot });
            }
        }

        // After hiding: a webview can only be frozen while it is hidden.
        effects.extend(self.settle_background());
        self.just_left.clear();
        effects
    }

    /// Reads and captures each page last shown whose tab is no longer visible
    /// and still has it running. A tab that closed or moved to an internal
    /// page has no page left to read.
    fn leave_shown(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        for (slot, tab) in self.shown.clone() {
            let running = self
                .tab(tab)
                .is_ok_and(|found| found.presence == TabPresence::Live { slot });
            if self.is_visible(tab) || !running {
                continue;
            }
            self.just_left.insert(tab);
            effects.push(Effect::Leave {
                slot,
                tab,
                capture: true,
            });
        }
        effects
    }

    /// Applies the discard and freeze policies to every background tab, then
    /// lets go of parked webviews beyond the spare.
    fn settle_background(&mut self) -> Vec<Effect> {
        let background = self.background();
        let discards = if self.discard == Policy::Smart {
            self.smart_discards(&background)
        } else {
            HashSet::new()
        };
        let relieving = self.pressure != Pressure::Normal;

        let mut effects = Vec::new();
        for id in background {
            if discards.contains(&id) {
                effects.extend(self.discard_tab(id));
                if let Ok(tab) = self.tab_mut(id) {
                    tab.relieved = relieving;
                }
            } else {
                effects.extend(self.settle(id));
            }
        }
        effects.extend(self.trim_parked());
        effects
    }

    /// The background tabs smart discarding gives up, by what each would lose,
    /// how recently it was left, the budget and the pressure level.
    ///
    /// At normal pressure a tab in grace is kept, one that would lose nothing
    /// is discarded, one holding work is kept, and those that would lose state
    /// are kept most recently shown first while all kept tabs fit the budget.
    /// Tight pressure ends grace and discards state; critical pressure
    /// discards work too. Must-run tabs and tabs paused on a dialog are never
    /// candidates.
    fn smart_discards(&self, background: &[TabId]) -> HashSet<TabId> {
        let mut discards = HashSet::new();
        let mut spent: u64 = 0;
        let mut contested: Vec<(TabId, u64, u64)> = Vec::new();

        for &id in background {
            let Ok(tab) = self.tab(id) else { continue };
            if self.must_run(tab) || tab.dialog.is_some() {
                continue;
            }
            let bytes = tab.slot().and_then(|slot| self.memory.get(&slot)).copied().unwrap_or(0);
            let loss = self.loss(tab);
            let in_grace = tab.active_at == 0 || self.clock.saturating_sub(tab.active_at) < GRACE_MS;

            let kept = match self.pressure {
                Pressure::Normal => in_grace || loss == Loss::Work,
                Pressure::Tight => loss == Loss::Work,
                Pressure::Critical => false,
            };
            if kept {
                // Counted against the budget, but not discarded for exceeding it.
                spent = spent.saturating_add(bytes);
            } else if self.pressure == Pressure::Normal && loss == Loss::State {
                contested.push((id, tab.active_at, bytes));
            } else {
                discards.insert(id);
            }
        }

        // Most recently shown first; once one does not fit, the rest go too.
        contested.sort_by_key(|&(_, active_at, _)| Reverse(active_at));
        let mut within = true;
        for (id, _, bytes) in contested {
            within = within && spent.saturating_add(bytes) <= self.budget;
            if within {
                spent += bytes;
            } else {
                discards.insert(id);
            }
        }
        discards
    }

    /// Destroys parked webviews beyond one warm spare, or all of them under
    /// pressure. A spare saves creating a webview on the next reload; more
    /// than one only holds memory.
    fn trim_parked(&mut self) -> Vec<Effect> {
        let spare = usize::from(self.pressure == Pressure::Normal);
        let parked: Vec<SlotId> = self
            .pool
            .slots()
            .iter()
            .filter(|slot| slot.occupant.is_none())
            .map(|slot| slot.id)
            .skip(spare)
            .collect();
        parked
            .into_iter()
            .filter_map(|slot| {
                self.pool.remove(slot)?;
                self.drop_slot(slot);
                Some(Effect::Destroy { slot })
            })
            .collect()
    }

    /// Tabs holding a slot that the optimization policies may act on: neither
    /// visible nor fixed.
    fn background(&self) -> Vec<TabId> {
        self.tabs
            .iter()
            .filter(|tab| tab.slot().is_some() && !tab.fixed && !self.is_visible(tab.id))
            .map(|tab| tab.id)
            .collect()
    }

    /// Applies the freeze and discard policies to one background tab that smart
    /// discarding keeps.
    ///
    /// A tab that must run is left running; see [`Browser::must_run`] for why
    /// that does not hold under an _always_ policy.
    fn settle(&mut self, id: TabId) -> Vec<Effect> {
        let Ok(tab) = self.tab(id) else { return Vec::new() };
        let keep_running = self.must_run(tab);
        // A page paused on a dialog is already still, and the dialog is waiting
        // on an answer the page has to be running to receive.
        let waiting = tab.dialog.is_some();

        match (self.discard, self.freeze) {
            (Policy::Always, _) if !self.has_connected(id) => self.discard_tab(id),
            (_, Policy::Never) => self.resume_tab(id),
            _ if keep_running => self.resume_tab(id),
            _ if waiting => Vec::new(),
            _ => self.freeze_tab(id),
        }
    }

    fn freeze_tab(&mut self, id: TabId) -> Vec<Effect> {
        let Ok(TabPresence::Live { slot }) = self.tab(id).map(|tab| tab.presence) else {
            return Vec::new();
        };
        let mut effects: Vec<Effect> = self.leave(id).into_iter().collect();
        if let Ok(tab) = self.tab_mut(id) {
            tab.presence = TabPresence::Frozen { slot };
        }
        effects.push(Effect::Freeze { slot });
        effects
    }

    /// Reads a running page before it is left. A frozen page was read when it
    /// was frozen, and cannot have changed since, and a page just read as it
    /// stopped being visible is not read again.
    fn leave(&self, id: TabId) -> Option<Effect> {
        if self.just_left.contains(&id) {
            return None;
        }
        match self.tab(id).ok()?.presence {
            TabPresence::Live { slot } => Some(Effect::Leave {
                slot,
                tab: id,
                capture: false,
            }),
            _ => None,
        }
    }

    fn resume_tab(&mut self, id: TabId) -> Vec<Effect> {
        let Ok(tab) = self.tab_mut(id) else { return Vec::new() };
        let TabPresence::Frozen { slot } = tab.presence else {
            return Vec::new();
        };
        tab.presence = TabPresence::Live { slot };
        vec![Effect::Resume { slot }]
    }

    /// Gives up a tab's page, keeping its place in history.
    fn discard_tab(&mut self, id: TabId) -> Vec<Effect> {
        let mut effects: Vec<Effect> = self.dismiss_dialog(id).into_iter().collect();
        effects.extend(self.leave(id));
        effects.extend(self.release_slot(id));
        if let Ok(tab) = self.tab_mut(id) {
            tab.lose_page();
        }
        self.connected.remove(&id);
        effects
    }

    /// Takes a tab's slot back and parks it, resuming it first if it was
    /// frozen. Leaves the tab's presence to the caller.
    fn release_slot(&mut self, id: TabId) -> Vec<Effect> {
        let frozen = matches!(self.tab(id).map(|tab| tab.presence), Ok(TabPresence::Frozen { .. }));
        let Some(slot) = self.pool.release(id) else {
            return Vec::new();
        };
        self.forget_slot(slot);
        // Parking stops whatever the page was playing.
        if let Ok(tab) = self.tab_mut(id) {
            tab.audible = false;
        }
        let mut effects = Vec::new();
        if frozen {
            effects.push(Effect::Resume { slot });
        }
        effects.extend([Effect::Hide { slot }, Effect::Blank { slot }]);
        effects
    }

    /// Ensures one tab is backed by a loaded webview.
    fn bind(&mut self, id: TabId) -> Vec<Effect> {
        let Ok(tab) = self.tab(id) else { return Vec::new() };
        if tab.is_internal() {
            return Vec::new();
        }

        let url = tab.url().to_string();
        let scroll = tab.scroll;
        let draft = tab.draft.clone();
        let height = tab
            .page
            .state
            .as_ref()
            .map(|state| state.height)
            .filter(|height| *height > 0.0);
        let had_slot = tab.slot().is_some();
        if let Some(request) = self.adopting.get(&id).copied().filter(|_| !had_slot) {
            return self.bind_new(id, request, url);
        }
        let effective = self.effective_capacity();
        let victim = self.victim();

        let Ok(acquired) = self.pool.acquire(id, effective, victim) else {
            // Every slot is spoken for; the tab stays discarded until one frees.
            return Vec::new();
        };

        let mut effects = Vec::new();
        let slot = acquired.slot();
        if let Acquired::Evicted { evicted, .. } = acquired {
            // The evicted page may be paused on a dialog; it has to be released
            // before its slot is navigated to this tab.
            effects.extend(self.dismiss_dialog(evicted));
            effects.extend(self.leave(evicted));
            if let Ok(tab) = self.tab_mut(evicted) {
                if matches!(tab.presence, TabPresence::Frozen { .. }) {
                    effects.push(Effect::Resume { slot });
                }
                tab.lose_page();
            }
            self.connected.remove(&evicted);
        }

        if let Ok(tab) = self.tab_mut(id) {
            if matches!(tab.presence, TabPresence::Frozen { .. }) {
                effects.push(Effect::Resume { slot });
            }
            tab.presence = TabPresence::Live { slot };
            tab.relieved = false;
        }

        let window = self.window_of(id).unwrap_or_else(|| self.main_window());
        if self.slot_windows.insert(slot, window).is_some_and(|was| was != window) {
            effects.push(Effect::Move { slot, window });
        }

        if self.slot_urls.get(&slot) != Some(&url) {
            self.slot_urls.insert(slot, url.clone());
            self.awaiting.insert(slot);
            effects.push(Effect::EnsureSlot {
                slot,
                url: url.clone(),
                window,
            });
            if !had_slot {
                if let Ok(tab) = self.tab_mut(id) {
                    tab.restoring = true;
                }
            }
            // Whatever slot the tab lands in, it gets back where it was.
            if scroll != Scroll::default() || draft.is_some() {
                effects.push(Effect::RestoreState {
                    slot,
                    url,
                    scroll,
                    height,
                    draft,
                });
            }
        }
        effects
    }

    /// Binds a connected tab to a slot that has never held a webview, then trims
    /// the pool back to capacity, as eviction would have.
    fn bind_new(&mut self, id: TabId, request: WindowRequestId, url: String) -> Vec<Effect> {
        let slot = self.pool.acquire_new(id);
        if let Ok(tab) = self.tab_mut(id) {
            tab.presence = TabPresence::Live { slot };
            tab.relieved = false;
        }
        let mut effects = self.trim();
        if self.pool.slots().len() > self.effective_capacity() {
            // Every other webview is protected. The tab does without one
            // rather than have the pool outgrow its capacity.
            self.pool.remove(slot);
            if let Ok(tab) = self.tab_mut(id) {
                tab.lose_page();
            }
            self.connected.remove(&id);
            return effects;
        }
        let window = self.window_of(id).unwrap_or_else(|| self.main_window());
        self.slot_windows.insert(slot, window);
        self.slot_urls.insert(slot, url.clone());
        self.awaiting.insert(slot);
        // Nothing was ever shown in a new webview, so nothing is covered
        // while the page loads.
        effects.push(Effect::Adopt {
            slot,
            request,
            url,
            window,
        });
        effects
    }
}

/// A site's conventional favicon location.
///
/// Reading the page's own `<link rel="icon">` would mean running a script inside
/// it and letting it report back. The conventional path costs nothing, is right
/// for most sites, and the interface already falls back to a generic icon when
/// the image fails to load.
pub fn favicon_for(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let scheme = if url.starts_with("https://") { "https" } else { "http" };
    let host = rest.split('/').next().filter(|host| !host.is_empty())?;
    Some(format!("{scheme}://{host}/favicon.ico"))
}

pub fn resolve_target(input: &str, search_url: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // An extension's own pages are reached by address: the engine draws no
    // toolbar to open its popup from.
    if is_internal(trimmed)
        || trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("chrome-extension://")
    {
        return trimmed.to_string();
    }
    // A single token with a dot and no spaces is a host, anything else is a query.
    if !trimmed.contains(' ') && trimmed.contains('.') {
        return format!("https://{trimmed}");
    }
    format!("{search_url}{}", urlencode(trimmed))
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
