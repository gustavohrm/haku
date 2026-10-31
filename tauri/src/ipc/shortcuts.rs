//! Shortcut ownership is captured on receipt; targets are resolved in queue order.

use crate::browser::{Browser, Effect, Pick};
use crate::error::Result;
use crate::model::{Shortcut, SlotId, TabId, WindowId, WindowKind};

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
    pub close: Option<WindowId>,
}

impl Request {
    pub fn chrome(window: WindowId) -> Self {
        Self { window, source: None }
    }

    /// The window the shortcut was pressed in.
    pub fn window(self) -> WindowId {
        self.window
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
        if browser.kind(self.window) == Some(WindowKind::Popup)
            && matches!(
                shortcut,
                Shortcut::ToggleSplit | Shortcut::MoveTabLeft | Shortcut::MoveTabRight | Shortcut::DuplicateTab
            )
        {
            return Ok(Action::default());
        }
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
            Shortcut::NewTab | Shortcut::NewWindow | Shortcut::NewPrivateWindow => {
                let (tab, effects) = if shortcut == Shortcut::NewTab {
                    browser.open_tab_in(self.window, home, true)
                } else if shortcut == Shortcut::NewPrivateWindow {
                    browser.open_private_window(home)
                } else {
                    browser.open_window(home)
                };
                action.address = Some(tab);
                effects
            }
            Shortcut::ToggleSplit => {
                if browser
                    .state_in(self.window)
                    .expect("focused window")
                    .splits
                    .iter()
                    .any(|pair| pair.tabs.contains(&tab))
                {
                    browser.separate_tabs(self.window, tab)?
                } else {
                    // Keep the selected operand until split_tab has paired the new partner.
                    let (partner, mut effects) = browser.open_tab_in(self.window, home, false);
                    effects.extend(browser.split_tab(self.window, partner)?);
                    action.address = Some(partner);
                    effects
                }
            }
            Shortcut::MoveTabLeft | Shortcut::MoveTabRight => {
                browser.move_tab(tab, shortcut == Shortcut::MoveTabRight)?;
                Vec::new()
            }
            Shortcut::CloseWindow => {
                action.close = Some(self.window);
                Vec::new()
            }
            Shortcut::Home => browser.navigate(tab, home)?,
            Shortcut::Memory => browser.open_tab_in(self.window, "haku://memory", true).1,
            Shortcut::DuplicateTab => {
                let history = &browser.tab(tab)?.history;
                if let Some(visit) = history.entries().get(history.index()) {
                    let url = visit.url.clone();
                    browser.open_tab_in(self.window, url, true).1
                } else {
                    Vec::new()
                }
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

    #[test]
    fn new_private_window_requests_the_address_of_its_private_home_tab() {
        let (mut browser, _) = pages(1);
        let action = Request::chrome(browser.main_window())
            .prepare(&mut browser, Shortcut::NewPrivateWindow, HOME, 1)
            .unwrap();
        let tab = action.address.expect("focus the new address field");
        let window = browser.window_of(tab).unwrap();
        assert!(browser.is_private(window));
        assert_eq!(browser.active_in(window), Some(tab));
        assert_eq!(browser.tab(tab).unwrap().url(), HOME);
    }

    fn pages(capacity: usize) -> (Browser, [TabId; 3]) {
        let mut browser = Browser::new(capacity);
        let (a, _) = browser.open_tab("https://a.test", true);
        let (b, _) = browser.open_tab("https://b.test", false);
        let (c, _) = browser.open_tab("https://c.test", false);
        (browser, [a, b, c])
    }

    fn run(browser: &mut Browser, chord: crate::model::Chord) -> Action {
        let shortcut = crate::model::shortcut_for(chord).expect("approved shortcut");
        Request::chrome(browser.main_window())
            .prepare(browser, shortcut, HOME, 1)
            .unwrap()
    }

    fn split_chord() -> crate::model::Chord {
        crate::model::Chord {
            shift: true,
            ..crate::model::Chord::alt(crate::model::Key::Letter('N'))
        }
    }

    #[test]
    fn queued_new_shortcuts_use_focus_instead_of_the_replaced_source_page() {
        let (mut browser, [a, b, _]) = pages(2);
        let request = Request::page(&browser, browser.tab(a).unwrap().slot().unwrap()).unwrap();
        browser.select_tab(b, 1).unwrap();
        request.prepare(&mut browser, Shortcut::ToggleSplit, HOME, 2).unwrap();
        let partner = browser.active().unwrap();
        assert_eq!(browser.state().splits[0].tabs, [b, partner]);
        request.prepare(&mut browser, Shortcut::ToggleSplit, HOME, 3).unwrap();
        assert!(browser.state().splits.is_empty());
        request.prepare(&mut browser, Shortcut::DuplicateTab, HOME, 4).unwrap();
        assert_eq!(browser.tab(browser.active().unwrap()).unwrap().url(), HOME);
        assert_ne!(browser.active(), Some(partner));
    }

    #[test]
    fn pair_moves_over_another_pair_without_breaking_either_join() {
        use crate::model::{Chord, Key};
        let (mut browser, [a, b, c]) = pages(2);
        let (d, _) = browser.open_tab("https://d.test", false);
        browser.split_tab(browser.main_window(), b).unwrap();
        browser.select_tab(c, 1).unwrap();
        browser.split_tab(browser.main_window(), d).unwrap();
        run(&mut browser, Chord::ctrl_shift(Key::PageUp));
        assert_eq!(
            browser.state().tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
            [c, d, a, b]
        );
        run(&mut browser, Chord::ctrl_shift(Key::PageUp));
        assert_eq!(browser.state().tabs[0].id, c);
        run(&mut browser, Chord::ctrl_shift(Key::PageDown));
        run(&mut browser, Chord::ctrl_shift(Key::PageDown));
        assert_eq!(
            browser.state().tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
            [a, b, c, d]
        );
        assert_eq!(browser.active(), Some(d));
    }

    #[test]
    fn close_window_keeps_the_origin_even_after_another_window_is_focused() {
        let (mut browser, [a, _, _]) = pages(1);
        let window = browser.window_of(a).unwrap();
        let request = Request::page(&browser, browser.tab(a).unwrap().slot().unwrap()).unwrap();
        browser.open_window(HOME);
        assert_eq!(
            request
                .prepare(&mut browser, Shortcut::CloseWindow, HOME, 1)
                .unwrap()
                .close,
            Some(window)
        );
    }

    #[test]
    fn home_navigates_the_focused_tab_to_the_configured_url() {
        let (mut browser, [a, b, _]) = pages(2);
        let request = Request::page(&browser, browser.tab(a).unwrap().slot().unwrap()).unwrap();
        browser.select_tab(b, 1).unwrap();
        request
            .prepare(&mut browser, Shortcut::Home, "https://configured.test/home", 2)
            .unwrap();
        assert_eq!(browser.tab(b).unwrap().url(), "https://configured.test/home");
        assert_eq!(browser.tab(a).unwrap().url(), "https://a.test");
    }

    #[test]
    fn memory_opens_an_internal_foreground_tab() {
        let (mut browser, _) = pages(1);
        let request = Request::chrome(browser.main_window());
        let action = request.prepare(&mut browser, Shortcut::Memory, HOME, 1).unwrap();
        assert_eq!(browser.tab(browser.active().unwrap()).unwrap().url(), "haku://memory");
        assert_eq!(browser.state().tabs.len(), 4);
        assert!(action.address.is_none());
    }

    #[test]
    fn popup_split_move_and_duplicate_do_nothing_but_close_names_the_popup() {
        use crate::browser::Opening;
        use crate::model::{Placement, WindowRequestId};
        let (mut browser, [a, _, _]) = pages(2);
        let (popup, _) = browser.open_from(
            browser.tab(a).unwrap().slot().unwrap(),
            "https://popup.test",
            Opening::Popup {
                request: WindowRequestId(1),
                placement: Placement::default(),
            },
        );
        let window = browser.window_of(popup).unwrap();
        let request = Request::chrome(window);
        let before = serde_json::to_value(browser.state_in(window)).unwrap();
        for shortcut in [
            Shortcut::ToggleSplit,
            Shortcut::MoveTabLeft,
            Shortcut::MoveTabRight,
            Shortcut::DuplicateTab,
        ] {
            let action = request.prepare(&mut browser, shortcut, HOME, 1).unwrap();
            assert!(action.effects.is_empty());
            assert!(action.address.is_none());
            assert_eq!(serde_json::to_value(browser.state_in(window)).unwrap(), before);
        }
        assert_eq!(
            request
                .prepare(&mut browser, Shortcut::CloseWindow, HOME, 1)
                .unwrap()
                .close,
            Some(window)
        );
    }

    #[test]
    fn split_toggle_creates_a_focused_right_partner_and_requests_its_address() {
        let (mut browser, [a, b, c]) = pages(2);
        let action = run(&mut browser, split_chord());
        let state = browser.state();
        let partner = state.active.unwrap();
        assert_ne!(partner, a);
        assert_eq!(action.address, Some(partner));
        assert_eq!(browser.tab(partner).unwrap().url(), HOME);
        assert_eq!(state.splits[0].tabs, [a, partner]);
        assert_eq!(state.splits[0].ratio, 0.5);
        assert_eq!(
            state.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
            [a, partner, b, c]
        );
    }

    #[test]
    fn split_toggle_separates_from_either_member_without_closing() {
        for left in [true, false] {
            let (mut browser, [a, b, _]) = pages(2);
            browser.split_tab(browser.main_window(), b).unwrap();
            let focused = if left { a } else { b };
            browser.select_tab(focused, 1).unwrap();
            let action = run(&mut browser, split_chord());
            assert!(browser.state().splits.is_empty());
            assert_eq!(browser.state().tabs.len(), 3);
            assert_eq!(browser.active(), Some(focused));
            assert!(action.address.is_none());
        }
    }

    #[test]
    fn move_shortcuts_step_over_pairs_and_stop_at_edges() {
        use crate::model::{Chord, Key};
        let (mut browser, [a, b, c]) = pages(2);
        browser.split_tab(browser.main_window(), b).unwrap();
        browser.select_tab(c, 1).unwrap();
        run(&mut browser, Chord::ctrl_shift(Key::PageUp));
        assert_eq!(
            browser.state().tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
            [c, a, b]
        );
        run(&mut browser, Chord::ctrl_shift(Key::PageUp));
        assert_eq!(browser.state().tabs[0].id, c);
        run(&mut browser, Chord::ctrl_shift(Key::PageDown));
        assert_eq!(
            browser.state().tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
            [a, b, c]
        );
        run(&mut browser, Chord::ctrl_shift(Key::PageDown));
        assert_eq!(browser.state().tabs[2].id, c);
    }

    #[test]
    fn move_shortcuts_move_either_pair_member_as_one_unit() {
        use crate::model::{Chord, Key};
        for member in [0, 1] {
            let (mut browser, [a, b, c]) = pages(2);
            browser.split_tab(browser.main_window(), b).unwrap();
            browser.select_tab([a, b][member], 1).unwrap();
            run(&mut browser, Chord::ctrl_shift(Key::PageDown));
            assert_eq!(
                browser.state().tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
                [c, a, b]
            );
            run(&mut browser, Chord::ctrl_shift(Key::PageUp));
            assert_eq!(
                browser.state().tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
                [a, b, c]
            );
        }
    }

    #[test]
    fn duplicate_opens_the_current_url_as_an_unpaired_foreground_tab() {
        use crate::model::{Chord, Key};
        let (mut browser, [_, b, _]) = pages(2);
        browser.split_tab(browser.main_window(), b).unwrap();
        run(&mut browser, Chord::ctrl_shift(Key::Letter('K')));
        let active = browser.active().unwrap();
        assert_ne!(active, b);
        assert_eq!(browser.tab(active).unwrap().url(), "https://b.test");
        assert_eq!(browser.state().tabs.len(), 4);
        assert_eq!(browser.state().splits.len(), 1);
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
