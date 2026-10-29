//! Shortcut ownership is captured on receipt; targets are resolved in queue order.

use crate::browser::{Browser, Effect, Pick};
use crate::error::Result;
use crate::model::{Shortcut, SlotId, TabId, WindowId};

#[derive(Clone, Copy)]
pub(super) struct Request {
    window: WindowId,
    source: Option<(SlotId, TabId)>,
}

#[derive(Default)]
pub(super) struct Action {
    pub effects: Vec<Effect>,
    pub address: Option<TabId>,
    pub devtools: Option<SlotId>,
}

impl Request {
    pub fn chrome(window: WindowId) -> Self {
        Self { window, source: None }
    }

    pub fn page(browser: &Browser, slot: SlotId) -> Option<Self> {
        let tab = browser.tab_in(slot)?;
        Some(Self {
            window: browser.window_of(tab)?,
            source: Some((slot, tab)),
        })
    }

    /// Called inside the serialized change turn, including native dispatch.
    pub fn prepare(self, browser: &mut Browser, shortcut: Shortcut, home: &str, now: u64) -> Result<Action> {
        let Some(focused) = browser.active_in(self.window) else {
            return Ok(Action::default());
        };
        let tab = match shortcut {
            Shortcut::CloseTab
            | Shortcut::Reload
            | Shortcut::Back
            | Shortcut::Forward
            | Shortcut::FocusAddress
            | Shortcut::DevTools => {
                if let Some((slot, tab)) = self.source {
                    if browser.tab_in(slot) != Some(tab) || browser.visible_tab_in(self.window, tab).is_err() {
                        return Ok(Action::default());
                    }
                    tab
                } else {
                    focused
                }
            }
            _ => focused,
        };
        let mut action = Action::default();
        let pick = match shortcut {
            Shortcut::NextTab => Some(Pick::Next),
            Shortcut::PreviousTab => Some(Pick::Previous),
            Shortcut::NthTab(index) => Some(Pick::Nth(index)),
            Shortcut::LastTab => Some(Pick::Last),
            _ => None,
        };
        if let Some(pick) = pick {
            if let Some(tab) = browser.pick_in(self.window, pick) {
                action.effects = browser.select_tab(tab, now)?;
            }
            return Ok(action);
        }
        action.effects = match shortcut {
            Shortcut::NewTab | Shortcut::NewWindow => {
                let (tab, effects) = if shortcut == Shortcut::NewTab {
                    browser.open_tab_in(self.window, home, true)
                } else {
                    browser.open_window(home)
                };
                action.address = Some(tab);
                effects
            }
            Shortcut::CloseTab => browser.close_tab(tab, home)?,
            Shortcut::ReopenClosedTab => browser.reopen_closed_tab(),
            Shortcut::Reload => browser.reload(tab)?,
            Shortcut::Back => browser.go_back(tab)?,
            Shortcut::Forward => browser.go_forward(tab)?,
            Shortcut::FocusAddress => {
                action.address = Some(tab);
                Vec::new()
            }
            Shortcut::DevTools => {
                action.devtools = browser.tab(tab)?.slot();
                Vec::new()
            }
            Shortcut::NextTab | Shortcut::PreviousTab | Shortcut::NthTab(_) | Shortcut::LastTab => unreachable!(),
        };
        Ok(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::Browser;
    use crate::model::{Shortcut, SlotId, TabId};

    const HOME: &str = "haku://new-tab";

    fn pages(capacity: usize) -> (Browser, [TabId; 3]) {
        let mut browser = Browser::new(capacity);
        let (a, _) = browser.open_tab("https://a.test", true);
        let (b, _) = browser.open_tab("https://b.test", false);
        let (c, _) = browser.open_tab("https://c.test", false);
        (browser, [a, b, c])
    }

    #[test]
    fn stale_page_shortcuts_never_act_on_a_slots_replacement() {
        for shortcut in [
            Shortcut::CloseTab,
            Shortcut::Reload,
            Shortcut::Back,
            Shortcut::Forward,
            Shortcut::FocusAddress,
            Shortcut::DevTools,
        ] {
            let (mut browser, [a, b, _]) = pages(1);
            let slot = browser.tab(a).unwrap().slot().unwrap();
            let request = Request::page(&browser, slot).unwrap();
            browser.select_tab(b, 1).unwrap();
            assert_eq!(browser.tab(b).unwrap().slot(), Some(slot));
            let before = serde_json::to_value(browser.state()).unwrap();
            let action = request.prepare(&mut browser, shortcut, HOME, 2).unwrap();
            assert!(action.effects.is_empty(), "{shortcut:?}");
            assert!(action.address.is_none(), "{shortcut:?}");
            assert!(action.devtools.is_none(), "{shortcut:?}");
            assert_eq!(serde_json::to_value(browser.state()).unwrap(), before);
        }
    }

    #[test]
    fn a_page_shortcut_is_dropped_when_its_tab_is_hidden_but_still_owns_its_slot() {
        let (mut browser, [a, b, _]) = pages(2);
        let slot = browser.tab(a).unwrap().slot().unwrap();
        let request = Request::page(&browser, slot).unwrap();
        browser.select_tab(b, 1).unwrap();
        assert_eq!(browser.tab(a).unwrap().slot(), Some(slot));
        let action = request.prepare(&mut browser, Shortcut::Reload, HOME, 2).unwrap();
        assert!(action.effects.is_empty());
    }

    #[test]
    fn consecutive_queued_page_ctrl_tabs_advance_from_each_result() {
        let (mut browser, [a, b, c]) = pages(1);
        let request = Request::page(&browser, browser.tab(a).unwrap().slot().unwrap()).unwrap();
        request.prepare(&mut browser, Shortcut::NextTab, HOME, 1).unwrap();
        assert_eq!(browser.active(), Some(b));
        request.prepare(&mut browser, Shortcut::NextTab, HOME, 2).unwrap();
        assert_eq!(browser.active(), Some(c));
    }

    #[test]
    fn chrome_shortcuts_resolve_the_focused_tab_when_dequeued() {
        let (mut browser, [_, b, _]) = pages(1);
        let request = Request::chrome(browser.main_window());
        request.prepare(&mut browser, Shortcut::NextTab, HOME, 1).unwrap();
        let action = request.prepare(&mut browser, Shortcut::FocusAddress, HOME, 2).unwrap();
        assert_eq!(action.address, Some(b));
    }

    #[test]
    fn page_address_focus_and_devtools_name_the_source_tab_and_slot() {
        let (mut browser, [a, _, _]) = pages(1);
        let slot = browser.tab(a).unwrap().slot().unwrap();
        let request = Request::page(&browser, slot).unwrap();
        assert_eq!(
            request
                .prepare(&mut browser, Shortcut::FocusAddress, HOME, 1)
                .unwrap()
                .address,
            Some(a)
        );
        assert_eq!(
            request
                .prepare(&mut browser, Shortcut::DevTools, HOME, 1)
                .unwrap()
                .devtools,
            Some(slot)
        );
    }

    #[test]
    fn an_unowned_slot_cannot_queue_page_shortcuts() {
        let (browser, _) = pages(1);
        assert!(Request::page(&browser, SlotId(usize::MAX)).is_none());
    }
}
