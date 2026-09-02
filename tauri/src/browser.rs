use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{HakuError, Result};
use crate::model::{is_internal, Acquired, Scroll, SlotId, Tab, TabId, TabPresence, Visit, WebviewPool};

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
    next_id: u64,
}

impl Browser {
    pub fn new(capacity: usize) -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            pool: WebviewPool::new(capacity),
            slot_urls: HashMap::new(),
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

        if activate || self.active.is_none() {
            self.active = Some(id);
        }
        (id, self.realize())
    }

    /// Closes a tab, moving activation to its right-hand neighbour.
    ///
    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn close_tab(&mut self, id: TabId) -> Result<Vec<Effect>> {
        let index = self.index_of(id)?;
        let mut effects = Vec::new();

        if let Some(slot) = self.pool.release(id) {
            effects.push(Effect::Hide { slot });
            effects.push(Effect::Blank { slot });
            self.slot_urls.remove(&slot);
        }
        self.tabs.remove(index);

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
        let tab = self.tab_mut(id)?;
        tab.history.push(Visit::new(url));
        tab.scroll = Scroll::default();
        tab.reclassify();

        // The tab may have just left the pool for an internal page.
        if self.tab(id)?.is_internal() {
            if let Some(slot) = self.pool.release(id) {
                self.slot_urls.remove(&slot);
                let mut effects = vec![Effect::Hide { slot }, Effect::Blank { slot }];
                effects.extend(self.realize());
                return Ok(effects);
            }
        }
        Ok(self.realize())
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
        let tab = self.tab_mut(id)?;
        let moved = if backwards { tab.history.go_back() } else { tab.history.go_forward() }.is_some();
        if !moved {
            return Ok(Vec::new());
        }
        tab.scroll = Scroll::default();
        tab.reclassify();
        Ok(self.realize())
    }

    /// # Errors
    /// Returns [`HakuError::TabNotFound`] when no tab has this id.
    pub fn reload(&mut self, id: TabId) -> Result<Vec<Effect>> {
        match self.tab(id)?.slot() {
            Some(slot) => Ok(vec![Effect::Reload { slot }]),
            None => Ok(self.realize()),
        }
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
                if let Ok(tab) = self.tab_mut(occupant) {
                    tab.presence = TabPresence::Suspended;
                }
            }
            self.slot_urls.remove(&slot);
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
            if let Some(slot) = self.pool.release(id) {
                self.slot_urls.remove(&slot);
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
    /// Both fields are optional because they arrive from different places: the
    /// page-load hook knows the URL, and the title only becomes known once the
    /// document sets it.
    ///
    /// A page can redirect, so a reported URL replaces the current entry rather
    /// than pushing a new one. Only navigation the user asked for grows the
    /// history stack.
    ///
    /// @returns The URL and title now displayed, for recording as a visit.
    pub fn report_page(&mut self, slot: SlotId, url: Option<String>, title: Option<String>) -> Option<(String, String)> {
        let id = self.occupant_of(slot)?;
        if let Some(url) = &url {
            self.slot_urls.insert(slot, url.clone());
        }

        let favicon = url.as_deref().and_then(favicon_for);
        let tab = self.tab_mut(id).ok()?;
        tab.history.update_current(url, title, favicon);

        let current = tab.history.current();
        Some((current.url.clone(), current.title.clone()))
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
