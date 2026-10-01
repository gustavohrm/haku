use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{HakuError, Result};
use crate::model::{
    is_internal, Acquired, Commit, DialogAnswer, DialogId, DialogKind, NavigationKind, PageDialog, Scroll, SlotId,
    Tab, TabId, TabPresence, Visit, WebviewPool,
};

/// URL a slot is parked on after its tab is suspended.
///
/// Navigating away frees the page while keeping the webview itself alive, which
/// is far cheaper than destroying and recreating one on every tab switch.
pub const BLANK_URL: &str = "about:blank";

/// A side effect the runtime must apply to a real webview.
///
/// [`Browser`] never touches Tauri. It decides what should be true and returns
/// the difference, which keeps the whole tab and pool policy testable without a
/// window, a webview, or an event loop.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Create the slot's webview if it does not exist, otherwise navigate it.
    EnsureSlot { slot: SlotId, url: String },
    /// Park the slot on [`BLANK_URL`] to release the page it was holding.
    Blank { slot: SlotId },
    Destroy { slot: SlotId },
    Show { slot: SlotId },
    Hide { slot: SlotId },
    Reload { slot: SlotId },
    RestoreScroll { slot: SlotId, scroll: Scroll },
    /// Release a page paused on a dialog, with the given answer.
    AnswerDialog { id: DialogId, answer: DialogAnswer },
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
    /// a navigation Haku made. A suspended tab reloading what it already
    /// showed is not a visit, and neither is a redirect, a replaced route or a
    /// retitle; those only refine the visit already recorded.
    pub visited: bool,
}

/// The projection of browser state the frontend renders.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub tabs: Vec<Tab>,
    pub active: Option<TabId>,
    /// Exported as a JavaScript number: every value here is a counter, a
    /// small capacity, or a millisecond timestamp, all far inside the range a
    /// double represents exactly.
    #[specta(type = specta_typescript::Number)]
    pub capacity: usize,
    #[specta(type = specta_typescript::Number)]
    pub live_count: usize,
}

pub struct Browser {
    tabs: Vec<Tab>,
    active: Option<TabId>,
    pool: WebviewPool,
    /// What each slot most recently loaded, so a redundant navigation is not
    /// emitted when a tab returns to a slot it never left.
    slot_urls: HashMap<SlotId, String>,
    /// Slots Haku has just navigated. The next URL such a slot commits is that
    /// navigation arriving, possibly redirected, and belongs to the entry that
    /// is already current rather than a new one.
    awaiting: HashSet<SlotId>,
    /// Tabs sent somewhere new whose page has not arrived yet. Their next
    /// awaited commit is a visit; any other awaited commit is a suspended tab
    /// coming back, which is not.
    navigated: HashSet<TabId>,
    next_id: u64,
}

impl Browser {
    pub fn new(capacity: usize) -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            pool: WebviewPool::new(capacity),
            slot_urls: HashMap::new(),
            awaiting: HashSet::new(),
            navigated: HashSet::new(),
            next_id: 1,
        }
    }

    /// Rebuilds a browser from persisted tabs, touching no webviews.
    ///
    /// Restoring must not go through the ordinary open and select path: that
    /// path assumes its effects will be applied to real webviews, and at startup
    /// there are none and nowhere to put them. A browser built here has every
    /// tab suspended, which is exactly what reconciling against the first
    /// reported layout then acts on.
    pub fn restored(capacity: usize, tabs: impl IntoIterator<Item = (String, bool)>, active: Option<usize>) -> Self {
        let mut browser = Self::new(capacity);

        for (url, fixed) in tabs {
            let id = TabId(browser.next_id);
            browser.next_id += 1;
            let mut tab = Tab::new(id, url);
            tab.fixed = fixed;
            browser.tabs.push(tab);
        }

        browser.active = active
            .and_then(|index| browser.tabs.get(index))
            .or_else(|| browser.tabs.first())
            .map(|tab| tab.id);
        browser
    }

    pub fn state(&self) -> BrowserState {
        BrowserState {
            tabs: self.tabs.clone(),
            active: self.active,
            capacity: self.pool.capacity(),
            live_count: self.tabs.iter().filter(|tab| tab.slot().is_some()).count(),
        }
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active(&self) -> Option<TabId> {
        self.active
    }

    pub fn capacity(&self) -> usize {
        self.pool.capacity()
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

    /// Drops what the browser remembered about a slot it has let go of.
    fn forget_slot(&mut self, slot: SlotId) {
        self.slot_urls.remove(&slot);
        self.awaiting.remove(&slot);
    }

    /// Releases a tab's pending dialog because its page is going away.
    fn dismiss_dialog(&mut self, id: TabId) -> Option<Effect> {
        let dialog = self.tab_mut(id).ok()?.dialog.take()?;
        Some(Effect::AnswerDialog { id: dialog.id, answer: dialog.abandoned() })
    }

    fn fixed_count(&self) -> usize {
        self.tabs.iter().filter(|tab| tab.fixed && !tab.is_internal()).count()
    }

    /// Tabs that must not lose their slot: the active tab and every fixed tab.
    fn protected(&self) -> Vec<TabId> {
        let mut protected: Vec<TabId> = self.tabs.iter().filter(|tab| tab.fixed).map(|tab| tab.id).collect();
        if let Some(active) = self.active {
            protected.push(active);
        }
        protected
    }

    // -- mutations -------------------------------------------------------

    pub fn open_tab(&mut self, url: impl Into<String>, activate: bool) -> (TabId, Vec<Effect>) {
        let id = TabId(self.next_id);
        self.next_id += 1;
        self.tabs.push(Tab::new(id, url));
        self.navigated.insert(id);

        if activate || self.active.is_none() {
            self.active = Some(id);
        }
        (id, self.realize())
    }

    /// Closes a tab, moving activation to its right-hand neighbour.
    ///
    /// The browser always keeps a tab: closing the last one opens `replacement`
    /// in its place, the page a new tab would show.
    ///
    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn close_tab(&mut self, id: TabId, replacement: &str) -> Result<Vec<Effect>> {
        let index = self.index_of(id)?;
        let mut effects: Vec<Effect> = self.dismiss_dialog(id).into_iter().collect();

        if let Some(slot) = self.pool.release(id) {
            effects.push(Effect::Hide { slot });
            effects.push(Effect::Blank { slot });
            self.forget_slot(slot);
        }
        self.tabs.remove(index);
        self.navigated.remove(&id);

        if self.tabs.is_empty() {
            let (_, opened) = self.open_tab(replacement, true);
            effects.extend(opened);
            return Ok(effects);
        }

        if self.active == Some(id) {
            self.active = self.tabs.get(index).or_else(|| self.tabs.last()).map(|tab| tab.id);
        }

        effects.extend(self.realize());
        Ok(effects)
    }

    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn select_tab(&mut self, id: TabId, now: u64) -> Result<Vec<Effect>> {
        self.index_of(id)?;
        self.active = Some(id);
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
        tab.scroll = Scroll::default();
        tab.reclassify();
        self.navigated.insert(id);

        // The tab may have just left the pool for an internal page.
        if self.tab(id)?.is_internal() {
            if let Some(slot) = self.pool.release(id) {
                self.forget_slot(slot);
                let mut effects: Vec<Effect> = dismissed.into_iter().collect();
                effects.extend([Effect::Hide { slot }, Effect::Blank { slot }]);
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
        let can_move = if backwards { tab.history.can_go_back() } else { tab.history.can_go_forward() };
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
        tab.scroll = Scroll::default();
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

    /// Pins or unpins a tab so it keeps a webview while other tabs are suspended.
    ///
    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn set_fixed(&mut self, id: TabId, fixed: bool) -> Result<Vec<Effect>> {
        self.tab_mut(id)?.fixed = fixed;
        Ok(self.realize())
    }

    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn reorder_tab(&mut self, id: TabId, to: usize) -> Result<()> {
        let from = self.index_of(id)?;
        let tab = self.tabs.remove(from);
        self.tabs.insert(to.min(self.tabs.len()), tab);
        Ok(())
    }

    pub fn set_capacity(&mut self, capacity: usize) -> Vec<Effect> {
        self.pool.set_capacity(capacity);
        let effective = self.pool.effective_capacity(self.fixed_count());

        let mut effects = Vec::new();
        for (slot, occupant) in self.pool.shrink_to(effective) {
            if let Some(occupant) = occupant {
                effects.extend(self.dismiss_dialog(occupant));
                if let Ok(tab) = self.tab_mut(occupant) {
                    tab.presence = TabPresence::Suspended;
                }
            }
            self.forget_slot(slot);
            effects.push(Effect::Destroy { slot });
        }
        effects.extend(self.realize());
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

    /// Gives up the slots of fixed tabs that have been quiet for too long.
    ///
    /// Pinning promises a tab will not be reloaded while it is doing something.
    /// Once a page reports no activity there is nothing left to preserve, so the
    /// webview is worth more to another tab.
    pub fn release_idle_fixed(&mut self, now: u64, idle_after: u64) -> Vec<Effect> {
        let stale: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|tab| {
                tab.fixed
                    && Some(tab.id) != self.active
                    && tab.slot().is_some()
                    && now.saturating_sub(tab.active_at) >= idle_after
            })
            .map(|tab| tab.id)
            .collect();

        let mut effects = Vec::new();
        for id in stale {
            effects.extend(self.dismiss_dialog(id));
            if let Some(slot) = self.pool.release(id) {
                self.forget_slot(slot);
                effects.push(Effect::Hide { slot });
                effects.push(Effect::Blank { slot });
            }
            if let Ok(tab) = self.tab_mut(id) {
                tab.presence = TabPresence::Suspended;
            }
        }
        effects
    }

    // -- reports from the page -------------------------------------------

    fn occupant_of(&self, slot: SlotId) -> Option<TabId> {
        self.pool.slots().iter().find(|candidate| candidate.id == slot).and_then(|found| found.occupant)
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
            visited |= if awaited { self.navigated.remove(&id) } else { commit.kind == NavigationKind::Push };
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
                    tab.scroll = Scroll::default();
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
        Some(PageReport { url: current.url.clone(), title: current.title.clone(), visited })
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
            return vec![Effect::AnswerDialog { id: dialog.id, answer: dialog.abandoned() }];
        };

        // A page is paused while its dialog is open, so it cannot open a second
        // one; a stale one left behind is released rather than stranded.
        let mut effects: Vec<Effect> = self.dismiss_dialog(id).into_iter().collect();
        if let Ok(tab) = self.tab_mut(id) {
            tab.dialog = Some(dialog);
        } else {
            effects.push(Effect::AnswerDialog { id: dialog.id, answer: dialog.abandoned() });
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

    pub fn report_activity(&mut self, slot: SlotId, at: u64) {
        let Some(id) = self.occupant_of(slot) else {
            return;
        };
        if let Ok(tab) = self.tab_mut(id) {
            tab.active_at = at;
        }
    }

    // -- reconciliation ---------------------------------------------------

    /// Brings webviews in line with tab state and returns the difference.
    ///
    /// Every mutation ends here rather than emitting effects itself, so there is
    /// exactly one description of what "correct" looks like.
    fn realize(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let active = self.active;

        if let Some(active) = active {
            effects.extend(self.bind(active));
        }

        // Fixed tabs stay resident behind the active one, capacity permitting.
        let fixed: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|tab| tab.fixed && !tab.is_internal() && Some(tab.id) != active)
            .map(|tab| tab.id)
            .collect();
        for id in fixed {
            effects.extend(self.bind(id));
        }

        let active_slot = active.and_then(|id| self.tab(id).ok()).and_then(Tab::slot);
        for slot in self.pool.slots().iter().map(|slot| slot.id).collect::<Vec<_>>() {
            if Some(slot) == active_slot {
                effects.push(Effect::Show { slot });
            } else {
                effects.push(Effect::Hide { slot });
            }
        }
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
        let effective = self.pool.effective_capacity(self.fixed_count());
        let protected = self.protected();

        let Ok(acquired) = self.pool.acquire(id, effective, &protected) else {
            // Every slot is spoken for; the tab stays suspended until one frees.
            return Vec::new();
        };

        let mut effects = Vec::new();
        if let Acquired::Evicted { evicted, .. } = acquired {
            // The evicted page may be paused on a dialog; it has to be released
            // before its slot is navigated to this tab.
            effects.extend(self.dismiss_dialog(evicted));
            if let Ok(tab) = self.tab_mut(evicted) {
                tab.presence = TabPresence::Suspended;
            }
        }

        let slot = acquired.slot();
        if let Ok(tab) = self.tab_mut(id) {
            tab.presence = TabPresence::Live { slot };
        }

        if self.slot_urls.get(&slot) != Some(&url) {
            self.slot_urls.insert(slot, url.clone());
            self.awaiting.insert(slot);
            effects.push(Effect::EnsureSlot { slot, url });
            if scroll != Scroll::default() {
                effects.push(Effect::RestoreScroll { slot, scroll });
            }
        }
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
    if is_internal(trimmed) || trimmed.starts_with("http://") || trimmed.starts_with("https://") {
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
