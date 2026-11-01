//! Shortcut ownership is captured on receipt; targets are resolved in queue order.
//! Recording a chord for settings happens here too, since it takes the chords
//! the interface would otherwise run.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::async_runtime::{channel, Receiver, Sender};

use crate::browser::{Browser, Effect, Pick};
use crate::error::Result;
use crate::model::{Chord, ChordRefusal, Key, Shortcut, SlotId, TabId, WindowId, WindowKind};

/// A chord pressed for settings to bind.
/// [Shortcuts § Changing bindings](../../../docs/specs/shortcuts.md#changing-bindings).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecordedChord {
    pub chord: String,
    /// Why it cannot be bound, or nothing when it can.
    pub refusal: Option<ChordRefusal>,
}

impl From<Chord> for RecordedChord {
    fn from(chord: Chord) -> Self {
        Self {
            chord: chord.to_string(),
            refusal: chord.refusal(),
        }
    }
}

/// The recording pending in each window, answered by the next chord pressed
/// in its interface. Held only briefly: the key handler takes it on the UI
/// thread.
static RECORDINGS: LazyLock<Mutex<HashMap<WindowId, Sender<Option<Chord>>>>> = LazyLock::new(Mutex::default);

/// The chord each window's last recording took, whose repeats are consumed
/// while it is held: a held Alt+F4 would otherwise close the window.
static RECORDED: LazyLock<Mutex<HashMap<WindowId, Chord>>> = LazyLock::new(Mutex::default);

/// Starts recording in `window`, answering any recording already pending
/// there with nothing. The receiver gets the chord, or nothing when cancelled.
pub(super) fn start_recording(window: WindowId) -> Receiver<Option<Chord>> {
    let (sender, receiver) = channel(1);
    let previous = RECORDINGS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(window, sender);
    if let Some(previous) = previous {
        let _ = previous.try_send(None);
    }
    receiver
}

/// Answers the recording pending in `window`, if any, with nothing.
pub(super) fn cancel_recording(window: WindowId) {
    RECORDED.lock().unwrap_or_else(PoisonError::into_inner).remove(&window);
    let pending = RECORDINGS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&window);
    if let Some(pending) = pending {
        let _ = pending.try_send(None);
    }
}

/// Gives `chord` to the recording pending in `window`, and answers whether
/// one took it, or took this chord before and it is held. Plain Esc cancels
/// the recording. Keys without Ctrl, Alt or a function key reach the
/// interface as usual, so Tab still moves focus.
pub(super) fn record(window: WindowId, chord: Chord, repeat: bool) -> bool {
    let mut recorded = RECORDED.lock().unwrap_or_else(PoisonError::into_inner);
    if repeat {
        return recorded.get(&window) == Some(&chord);
    }
    recorded.remove(&window);
    let typed = !chord.ctrl && !chord.alt && !matches!(chord.key, Key::Function(_)) && chord.key != Key::Escape;
    if typed {
        return false;
    }
    let pending = RECORDINGS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&window);
    let Some(pending) = pending else {
        return false;
    };
    let _ = pending.try_send((chord != Chord::plain(Key::Escape)).then_some(chord));
    recorded.insert(window, chord);
    true
}

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
    pub page: Option<PageAction>,
}

/// What a shortcut asks of its tab's live page, done once the change commits.
/// [Find, print and save](../../../docs/specs/find-print-save.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PageAction {
    /// Open the find bar for the tab.
    Find(TabId),
    Print(SlotId),
    Save(SlotId),
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
            | Shortcut::DevTools
            | Shortcut::Find
            | Shortcut::Print
            | Shortcut::SavePage => {
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
            Shortcut::LastTab => Some(Pick::Last),
            _ => shortcut.tab_index().map(Pick::Nth),
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
            // Internal and crashed pages have no slot, and nothing to search,
            // print or save.
            Shortcut::Find | Shortcut::Print | Shortcut::SavePage => {
                action.page = browser.tab(tab)?.slot().map(|slot| match shortcut {
                    Shortcut::Find => PageAction::Find(tab),
                    Shortcut::Print => PageAction::Print(slot),
                    _ => PageAction::Save(slot),
                });
                Vec::new()
            }
            Shortcut::NextTab
            | Shortcut::PreviousTab
            | Shortcut::Tab1
            | Shortcut::Tab2
            | Shortcut::Tab3
            | Shortcut::Tab4
            | Shortcut::Tab5
            | Shortcut::Tab6
            | Shortcut::Tab7
            | Shortcut::Tab8
            | Shortcut::LastTab => unreachable!(),
        };
        Ok(action)
    }
}

#[cfg(test)]
mod recording_tests {
    use super::*;

    fn answer(receiver: &mut Receiver<Option<Chord>>) -> Option<Option<Chord>> {
        receiver.try_recv().ok()
    }

    /// A press, not a repeat.
    fn record_once(window: WindowId, chord: Chord) -> bool {
        record(window, chord, false)
    }

    #[test]
    fn a_recording_takes_the_next_chord_once() {
        let window = WindowId(9001);
        let mut receiver = start_recording(window);
        assert!(!record_once(window, Chord::plain(Key::Tab)), "Tab still moves focus");
        assert!(record_once(window, Chord::ctrl(Key::Letter('T'))));
        assert_eq!(answer(&mut receiver), Some(Some(Chord::ctrl(Key::Letter('T')))));
        assert!(
            !record_once(window, Chord::ctrl(Key::Letter('T'))),
            "the next press runs its shortcut"
        );
    }

    #[test]
    fn a_recorded_chord_held_down_is_consumed_until_another_press() {
        let window = WindowId(9005);
        let alt_f4 = Chord::alt(Key::Function(4));
        let mut receiver = start_recording(window);
        assert!(!record(window, alt_f4, true), "a repeat does not answer a recording");
        assert!(record_once(window, alt_f4));
        assert_eq!(answer(&mut receiver), Some(Some(alt_f4)));
        assert!(record(window, alt_f4, true), "held, it must not close the window");
        assert!(!record(window, Chord::ctrl(Key::Letter('T')), true));

        assert!(!record_once(window, alt_f4), "pressed again, it is no longer recorded");
        assert!(!record(window, alt_f4, true));
    }

    #[test]
    fn esc_another_recording_or_cancelling_answers_with_nothing() {
        let window = WindowId(9002);
        let mut first = start_recording(window);
        assert!(record_once(window, Chord::plain(Key::Escape)));
        assert_eq!(answer(&mut first), Some(None));

        let mut second = start_recording(window);
        let mut third = start_recording(window);
        assert_eq!(answer(&mut second), Some(None));
        cancel_recording(window);
        assert_eq!(answer(&mut third), Some(None));
    }

    #[test]
    fn shift_esc_and_refused_chords_are_recorded_with_their_refusal() {
        let window = WindowId(9003);
        let mut receiver = start_recording(window);
        assert!(record_once(window, Chord::shift(Key::Escape)));
        assert_eq!(answer(&mut receiver), Some(Some(Chord::shift(Key::Escape))));
        assert_eq!(
            RecordedChord::from(Chord::ctrl(Key::Letter('C'))),
            RecordedChord {
                chord: "Ctrl+C".into(),
                refusal: Some(ChordRefusal::Reserved),
            }
        );
    }

    #[test]
    fn a_window_not_recording_takes_nothing() {
        assert!(!record_once(WindowId(9004), Chord::ctrl(Key::Letter('T'))));
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
        let shortcut = crate::model::Keymap::default()
            .shortcut_for(chord)
            .expect("approved shortcut");
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
            Shortcut::Find,
            Shortcut::Print,
            Shortcut::SavePage,
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
            assert!(action.page.is_none(), "{shortcut:?}");
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
    fn find_print_and_save_act_on_the_source_page() {
        let (mut browser, [a, _, _]) = pages(1);
        let slot = browser.tab(a).unwrap().slot().unwrap();
        let request = Request::page(&browser, slot).unwrap();
        let page = |browser: &mut Browser, shortcut| request.prepare(browser, shortcut, HOME, 1).unwrap().page;
        assert_eq!(page(&mut browser, Shortcut::Find), Some(PageAction::Find(a)));
        assert_eq!(page(&mut browser, Shortcut::Print), Some(PageAction::Print(slot)));
        assert_eq!(page(&mut browser, Shortcut::SavePage), Some(PageAction::Save(slot)));
    }

    #[test]
    fn find_print_and_save_do_nothing_on_an_internal_page() {
        let mut browser = Browser::new(1);
        browser.open_tab("haku://settings", true);
        for shortcut in [Shortcut::Find, Shortcut::Print, Shortcut::SavePage] {
            let action = Request::chrome(browser.main_window())
                .prepare(&mut browser, shortcut, HOME, 1)
                .unwrap();
            assert!(action.page.is_none(), "{shortcut:?}");
            assert!(action.effects.is_empty(), "{shortcut:?}");
        }
    }

    #[test]
    fn an_unowned_slot_cannot_queue_page_shortcuts() {
        let (browser, _) = pages(1);
        assert!(Request::page(&browser, SlotId(usize::MAX)).is_none());
    }
}
