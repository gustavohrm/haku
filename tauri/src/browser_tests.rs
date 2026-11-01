use super::*;
use crate::model::page_state::INTERACTION_THRESHOLD;
use crate::model::Loss;

const SEARCH: &str = "https://duckduckgo.com/?q=";
const HOME: &str = "haku://new-tab";

#[test]
fn pressure_keeps_one_anchor_when_several_private_pages_are_discarded_together() {
    let mut browser = Browser::new(4);
    let (first, _) = browser.open_private_window("https://first.test");
    let (second, _) = browser.open_private_window("https://second.test");
    browser.slots_created(browser.slot_ids());
    browser.open_tab_in(browser.window_of(first).unwrap(), HOME, true);
    browser.open_tab_in(browser.window_of(second).unwrap(), HOME, true);
    browser.set_pressure(Pressure::Tight);
    browser.set_optimization(4, Policy::Always, Policy::Always);
    assert_eq!(
        browser
            .slots()
            .iter()
            .filter(|slot| slot.profile == Profile::Private)
            .count(),
        1
    );
}

#[test]
fn adopting_a_private_popup_creates_its_slot_before_destroying_the_old_anchor() {
    let mut browser = Browser::new(1);
    let (parent, _) = browser.open_private_window("https://secret.test");
    let anchor = browser.tab(parent).unwrap().slot().unwrap();
    browser.slots_created([anchor]);
    let window = browser.window_of(parent).unwrap();
    browser.open_tab_in(window, HOME, true);
    browser.close_tab(parent, HOME).unwrap();
    let (_, effects) = browser.open_requested_profile(
        None,
        Profile::Private,
        "https://popup.test",
        Opening::Popup {
            request: WindowRequestId(8),
            placement: Placement::default(),
        },
    );
    let replacement = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Adopt { slot, .. } => Some(*slot),
            _ => None,
        })
        .unwrap();
    assert!(!effects.contains(&Effect::Destroy { slot: anchor }));
    assert!(browser.slot_ids().contains(&anchor));
    let retirement = browser.slots_created([replacement]);
    assert!(retirement.contains(&Effect::Destroy { slot: anchor }));
    assert_eq!(browser.slot_ids(), [replacement]);
}

#[test]
fn failed_private_popup_creation_preserves_the_anchor_and_capacity() {
    let mut browser = Browser::new(1);
    let (parent, _) = browser.open_private_window("https://secret.test");
    let anchor = browser.tab(parent).unwrap().slot().unwrap();
    browser.slots_created([anchor]);
    let home = browser.window_of(parent).unwrap();
    browser.open_tab_in(home, HOME, true);
    browser.close_tab(parent, HOME).unwrap();
    let (popup, effects) = browser.open_requested_profile(
        None,
        Profile::Private,
        "https://popup.test",
        Opening::Popup {
            request: WindowRequestId(8),
            placement: Placement::default(),
        },
    );
    let failed = browser.tab(popup).unwrap().slot().unwrap();
    assert!(
        !effects.contains(&Effect::Destroy { slot: anchor }),
        "retirement must wait for native success"
    );
    let cleanup = browser.slots_failed([failed]);
    assert!(cleanup.contains(&Effect::Destroy { slot: failed }));
    assert!(!cleanup.contains(&Effect::Destroy { slot: anchor }));
    assert!(browser.slots().iter().any(|slot| slot.id == anchor));
    assert!(!browser.slot_ids().contains(&failed));
    assert!(!browser.slot_windows.contains_key(&failed));
    assert_eq!(browser.slots().len(), 1);
    assert!(browser.slots().len() <= browser.effective_capacity());
}

#[test]
fn failed_private_creation_is_removed_before_its_window_can_transfer_slots() {
    let mut browser = Browser::new(1);
    browser.open_private_window(HOME);
    let (tab, _) = browser.open_private_window("https://failed.test");
    let failed = browser.tab(tab).unwrap().slot().unwrap();
    let window = browser.window_of(tab).unwrap();
    let cleanup = browser.slots_failed([failed]);
    assert!(browser.state_in(window).is_none());
    assert!(
        !browser.slot_ids().contains(&failed),
        "a nonexistent slot must not move to the surviving window"
    );
    assert!(!browser.slot_windows.contains_key(&failed));
    assert!(!cleanup
        .iter()
        .any(|effect| matches!(effect, Effect::Move { slot, .. } if *slot == failed)));
}

#[test]
fn failed_private_siblings_are_removed_before_any_window_transfers_them() {
    let mut browser = Browser::new(2);
    let (left, _) = browser.open_private_window("https://left.test");
    let window = browser.window_of(left).unwrap();
    let (right, _) = browser.open_tab_in(window, "https://right.test", true);
    let failed = [
        browser.tab(left).unwrap().slot().unwrap(),
        browser.tab(right).unwrap().slot().unwrap(),
    ];
    let (survivor, _) = browser.open_private_window(HOME);
    let surviving_window = browser.window_of(survivor).unwrap();
    let cleanup = browser.slots_failed(failed);
    assert!(
        browser.state_in(surviving_window).is_some(),
        "failed slots must not be transferred and then close the healthy window"
    );
    assert!(!cleanup
        .iter()
        .any(|effect| matches!(effect, Effect::Move { slot, .. } if failed.contains(slot))));
    assert!(browser.slot_ids().iter().all(|slot| !failed.contains(slot)));
}

#[test]
fn ensuring_a_private_slot_retires_the_anchor_only_after_native_success() {
    for succeeds in [false, true] {
        let mut browser = Browser::new(2).with_policies(Policy::Always, Policy::Always);
        let (parent, _) = browser.open_private_window("https://secret.test");
        let anchor = browser.tab(parent).unwrap().slot().unwrap();
        browser.slots_created([anchor]);
        let home = browser.window_of(parent).unwrap();
        browser.open_private_window(HOME);
        browser.set_pressure(Pressure::Tight);
        // Binding the new visible page precedes discarding the old one. The
        // pressure trim must not destroy the old native session anchor yet.
        let (new, effects) = browser.open_tab_in(home, "https://new.test", true);
        let replacement = browser.tab(new).unwrap().slot().unwrap();
        assert_ne!(replacement, anchor);
        assert!(effects
            .iter()
            .any(|effect| matches!(effect, Effect::EnsureSlot { slot, .. } if *slot == replacement)));
        assert!(!effects.contains(&Effect::Destroy { slot: anchor }));
        if succeeds {
            let retirement = browser.slots_created([replacement]);
            assert!(retirement.contains(&Effect::Destroy { slot: anchor }));
            assert_eq!(browser.slot_ids(), [replacement]);
        } else {
            browser.slots_failed([replacement]);
            assert_eq!(browser.slot_ids(), [anchor]);
        }
        assert!(browser.slots().len() <= browser.effective_capacity());
    }
}

#[test]
fn private_anchor_does_not_hide_other_eviction_and_trimming_candidates() {
    for trim in [false, true] {
        let mut browser = Browser::new(3);
        let (private, _) = browser.open_private_window("https://secret.test");
        let window = browser.window_of(private).unwrap();
        browser.open_tab_in(window, HOME, true);
        let (old, _) = browser.open_tab("https://old.test", true);
        browser.open_tab("https://current.test", true);
        if trim {
            browser.set_capacity(2);
            assert_eq!(browser.slots().len(), 2);
        } else {
            let (new, _) = browser.open_tab("https://new.test", true);
            assert!(browser.tab(new).unwrap().slot().is_some());
        }
        assert!(browser.tab(old).unwrap().slot().is_none());
        assert!(browser.slots().iter().any(|slot| slot.profile == Profile::Private));
    }
}

#[test]
fn queued_private_requests_expire_with_the_private_session() {
    let mut browser = Browser::new(1);
    let (first, _) = browser.open_private_window(HOME);
    let epoch = browser.private_session();
    let (second, _) = browser.open_private_window(HOME);
    browser.close_window(browser.window_of(first).unwrap());
    assert!(browser.accepts_private_request(epoch));
    browser.close_window(browser.window_of(second).unwrap());
    assert!(!browser.accepts_private_request(epoch));
    browser.open_private_window(HOME);
    assert!(!browser.accepts_private_request(epoch));
}

#[test]
fn closing_a_private_windows_last_tab_exits_when_no_browser_window_remains() {
    let mut browser = Browser::new(1);
    let (private, _) = browser.open_private_window("https://secret.test");
    browser.close_window(WindowId(1));
    let effects = browser.close_tab(private, HOME).unwrap();
    assert!(effects.contains(&Effect::Quit));
    assert!(browser.tabs().is_empty());
    assert!(browser.slots().is_empty());
}

#[test]
fn private_tabs_cannot_supply_persisted_site_settings_or_extension_targets() {
    let mut browser = Browser::new(2);
    let (normal, _) = browser.open_tab("https://public.test", true);
    let (private, _) = browser.open_private_window("https://secret.test");
    let private_window = browser.window_of(private).unwrap();
    assert!(browser.allow_site_settings(private_window).is_err());
    assert!(browser.extension_tab_in(private_window, private).is_err());
    assert!(browser.allow_site_settings(browser.window_of(normal).unwrap()).is_ok());
    browser.close_window(private_window);
    assert!(
        browser.allow_site_settings(private_window).is_err(),
        "a queued command from a closed private chrome cannot persist site settings"
    );
}

#[test]
fn default_popup_tabs_never_route_into_the_last_used_private_window() {
    let mut browser = Browser::new(1);
    let (normal, _) = browser.open_tab("https://public.test", true);
    let slot = browser.tab(normal).unwrap().slot().unwrap();
    let (popup, _) = browser.open_from(
        slot,
        "https://public.test/login",
        Opening::Popup {
            request: WindowRequestId(77),
            placement: Placement::default(),
        },
    );
    let popup_window = browser.window_of(popup).unwrap();
    browser.open_private_window(HOME);
    let (child, _) = browser.open_tab_in(popup_window, HOME, true);
    assert!(!browser.is_private(browser.window_of(child).unwrap()));
}

#[test]
fn a_failed_private_slot_closes_its_window() {
    let mut browser = Browser::new(1);
    let (private, _) = browser.open_private_window("https://secret.test");
    let window = browser.window_of(private).unwrap();
    let slot = browser.tab(private).unwrap().slot().unwrap();
    let effects = browser.slots_failed([slot]);
    assert!(browser.state_in(window).is_none());
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::Destroy { slot: destroyed } if *destroyed == slot)));
}

#[test]
fn closing_the_last_private_tab_closes_its_window_and_destroys_the_session() {
    let mut browser = Browser::new(1);
    let (tab, _) = browser.open_private_window("https://secret.test");
    let window = browser.window_of(tab).unwrap();
    let effects = browser.close_tab(tab, HOME).unwrap();
    assert!(browser.state_in(window).is_none());
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::CloseWindow { window: closed } if *closed == window)));
    assert!(browser.slots().iter().all(|slot| slot.profile != Profile::Private));
}

#[test]
fn same_url_in_another_profile_requires_a_new_slot() {
    let mut browser = Browser::new(1);
    let (normal, _) = browser.open_tab("https://same.test", true);
    let old = browser.tab(normal).unwrap().slot().unwrap();
    browser.navigate(normal, HOME).unwrap();
    let (private, effects) = browser.open_private_window("https://same.test");
    let new = browser.tab(private).unwrap().slot().unwrap();
    assert_ne!(old, new);
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::Destroy { slot } if *slot == old)));
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::EnsureSlot { slot, profile: Profile::Private, .. } if *slot == new)));
}

#[test]
fn private_pages_never_offer_persistent_history_or_remote_favicons() {
    let mut browser = Browser::new(1);
    let (tab, _) = browser.open_private_window("https://secret.test");
    let window = browser.window_of(tab).unwrap();
    assert!(browser.state_in(window).unwrap().private);
    let slot = browser.tab(tab).unwrap().slot().unwrap();
    assert!(browser
        .report_page(
            slot,
            &[Commit {
                url: "https://secret.test".into(),
                kind: NavigationKind::Push
            }],
            Some("Secret".into())
        )
        .is_none());
    assert_eq!(browser.tab(tab).unwrap().history.current().title, "Secret");
    assert!(browser.tab(tab).unwrap().history.current().favicon.is_none());
    browser.open_tab_in(window, HOME, true);
    browser.close_tab(tab, HOME).unwrap();
    assert!(browser.reopen_closed_tab().is_empty());
}

#[test]
fn private_anchor_survives_discard_pressure_and_other_private_windows_closing() {
    let mut browser = Browser::new(1).with_policies(Policy::Always, Policy::Always);
    let (first, _) = browser.open_private_window("https://secret.test");
    let first_window = browser.window_of(first).unwrap();
    browser.open_tab_in(first_window, HOME, true);
    browser.tick(100_000, Pressure::Tight, HashMap::new());
    assert!(browser.tab(first).unwrap().slot().is_none());
    assert_eq!(
        browser
            .slots()
            .iter()
            .filter(|slot| slot.profile == Profile::Private)
            .count(),
        1
    );
    let (second, _) = browser.open_private_window(HOME);
    let second_window = browser.window_of(second).unwrap();
    browser.close_window(first_window);
    assert_eq!(
        browser
            .slots()
            .iter()
            .filter(|slot| slot.profile == Profile::Private)
            .count(),
        1
    );
    browser.close_window(second_window);
    assert!(browser.slots().iter().all(|slot| slot.profile == Profile::Default));
}

#[test]
fn private_popup_and_its_tabs_inherit_profile_even_after_opener_closes() {
    let mut browser = Browser::new(1);
    let (parent, _) = browser.open_private_window("https://secret.test");
    let home = browser.window_of(parent).unwrap();
    let slot = browser.tab(parent).unwrap().slot().unwrap();
    let (popup, effects) = browser.open_from(
        slot,
        "https://login.test",
        Opening::Popup {
            request: WindowRequestId(900),
            placement: Placement::default(),
        },
    );
    let popup_window = browser.window_of(popup).unwrap();
    assert!(browser.is_private(popup_window));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Adopt {
            profile: Profile::Private,
            ..
        }
    )));
    browser.focus(WindowId(1));
    let (child, _) = browser.open_tab_in(popup_window, HOME, true);
    assert_eq!(browser.window_of(child), Some(home));
    browser.close_window(home);
    let (child, _) = browser.open_tab_in(popup_window, HOME, true);
    assert!(browser.is_private(browser.window_of(child).unwrap()));
    let (late, _) = browser.open_requested_profile(None, Profile::Private, "https://late.test", Opening::Window);
    assert!(browser.is_private(browser.window_of(late).unwrap()));
}

#[test]
fn reselecting_the_current_tab_reconciles_background_grace_under_fresh_pressure() {
    let (mut browser, ids) = browser_with(3, &["https://background.test", "https://current.test"]);
    browser.set_optimization(3, Policy::Smart, Policy::Smart);
    assert!(slot_of(&browser, ids[0]).is_some());
    browser.set_pressure(Pressure::Tight);
    browser.select_tab(ids[1], 1).unwrap();
    assert_eq!(browser.tab(ids[0]).unwrap().presence, TabPresence::Discarded);
    assert!(slot_of(&browser, ids[1]).is_some());
}

#[test]
fn selecting_a_visible_sibling_reconciles_background_without_leaving_either_pane() {
    let (mut browser, ids) = browser_with(4, &["https://background.test", "https://left.test"]);
    let (_, left, right) = split_pair(&mut browser);
    browser.set_optimization(4, Policy::Smart, Policy::Smart);
    let visible_slots = [slot_of(&browser, left).unwrap(), slot_of(&browser, right).unwrap()];
    browser.set_pressure(Pressure::Tight);
    let effects = browser.select_tab(left, 1).unwrap();
    assert_eq!(browser.tab(ids[0]).unwrap().presence, TabPresence::Discarded);
    assert!(effects.iter().all(|effect| !matches!(effect,
        Effect::Leave { slot, .. } | Effect::Blank { slot } | Effect::Freeze { slot }
        if visible_slots.contains(slot)
    )));
}

#[test]
fn native_focus_maps_a_slot_to_its_window_and_viewport() {
    let (mut browser, _) = browser_with(2, &["https://a.test"]);
    let (tab, _) = browser.open_window("https://b.test");
    let window = browser.window_of(tab).unwrap();
    let slot = slot_of(&browser, tab).unwrap();
    let before = browser.state_in(window).unwrap();
    let target = browser.focus_slot(slot).unwrap();
    assert_eq!(target.window, window);
    assert_eq!(target.id, before.viewports[0].id);
    let after = browser.state_in(window).unwrap();
    assert_eq!(after.focused_viewport, target.id);
    assert_eq!(after.active, Some(tab));
    assert_eq!(after.layout_revision, before.layout_revision);
}

#[test]
fn native_focus_ignores_unknown_parked_and_hidden_slots() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.open_tab("https://b.test", true);
    assert!(browser.focus_slot(slot).is_none(), "a hidden tab has no assignment");
    browser.close_tab(ids[0], HOME).unwrap();
    assert!(browser.slot_ids().contains(&slot));
    assert!(browser.focus_slot(slot).is_none(), "a parked slot has no assignment");
    assert!(browser.focus_slot(SlotId(usize::MAX)).is_none());
}

#[test]
fn extension_popup_accepts_a_visible_tab_in_the_calling_window() {
    let (browser, ids) = browser_with(1, &[HOME]);
    assert_eq!(
        browser.visible_tab_in(browser.main_window(), ids[0]).unwrap().id,
        ids[0]
    );
}

#[test]
fn extension_popup_rejects_foreign_hidden_and_unknown_tabs_without_mutation() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let window = browser.main_window();
    let (hidden, _) = browser.open_tab("https://hidden.test", false);
    let (foreign, _) = browser.open_window("https://foreign.test");
    let before = serde_json::to_value(browser.state_in(window)).unwrap();
    for tab in [foreign, hidden, TabId(u64::MAX)] {
        assert!(browser.visible_tab_in(window, tab).is_err(), "accepted {tab:?}");
        assert_eq!(serde_json::to_value(browser.state_in(window)).unwrap(), before);
    }
    assert!(browser.visible_tab_in(window, ids[0]).is_ok());
}

fn assert_single_viewport(browser: &Browser, expected: TabId) -> (String, u64) {
    let state = serde_json::to_value(browser.state()).unwrap();
    let panes = state["viewports"].as_array().expect("export viewport assignments");
    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0]["tab"], serde_json::to_value(expected).unwrap());
    assert_eq!(state["active"], panes[0]["tab"]);
    assert_eq!(state["focusedViewport"], panes[0]["id"]);
    (
        panes[0]["id"].as_str().unwrap().into(),
        state["layoutRevision"].as_u64().unwrap(),
    )
}

#[test]
fn one_viewport_tracks_open_select_and_close_without_changing_its_id_or_revision() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let (pane, revision) = assert_single_viewport(&browser, ids[0]);
    let (background, _) = browser.open_tab("https://b.test", false);
    assert_eq!(assert_single_viewport(&browser, ids[0]), (pane.clone(), revision));
    browser.select_tab(background, 0).unwrap();
    let (selected_pane, selected_revision) = assert_single_viewport(&browser, background);
    assert_eq!(selected_pane, pane);
    assert_eq!(selected_revision, revision);
    browser.select_tab(background, 1).unwrap();
    assert_eq!(assert_single_viewport(&browser, background).1, selected_revision);
    browser.close_tab(background, HOME).unwrap();
    assert_eq!(assert_single_viewport(&browser, ids[0]).0, pane);
    let (foreground, _) = browser.open_tab("https://c.test", true);
    assert_eq!(assert_single_viewport(&browser, foreground).0, pane);
}

#[test]
fn closing_the_last_tab_keeps_the_viewport_assigned_to_the_home_replacement() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let (pane, revision) = assert_single_viewport(&browser, ids[0]);
    browser.close_tab(ids[0], HOME).unwrap();
    let replacement = browser.active().unwrap();
    assert_ne!(replacement, ids[0]);
    assert_eq!(browser.tab(replacement).unwrap().url(), HOME);
    let (new_pane, new_revision) = assert_single_viewport(&browser, replacement);
    assert_eq!(new_pane, pane);
    assert_eq!(new_revision, revision);
}

#[test]
fn assigning_the_first_tab_advances_the_layout_revision() {
    let mut browser = Browser::new(1);
    let revision = browser.state().layout_revision;
    browser.open_tab("https://a.test", true);
    assert_eq!(browser.state().layout_revision, revision + 1);
}

fn browser_with(capacity: usize, urls: &[&str]) -> (Browser, Vec<TabId>) {
    let mut browser = Browser::new(capacity);
    let ids = urls.iter().map(|url| browser.open_tab(*url, true).0).collect();
    (browser, ids)
}

fn slot_of(browser: &Browser, id: TabId) -> Option<SlotId> {
    browser.tab(id).unwrap().slot()
}

#[test]
fn the_first_tab_opened_becomes_active() {
    let (browser, ids) = browser_with(1, &["https://a.test"]);
    assert_eq!(browser.active(), Some(ids[0]));
}

#[test]
fn opening_a_tab_in_the_background_leaves_the_active_tab_alone() {
    let mut browser = Browser::new(1);
    let (first, _) = browser.open_tab("https://a.test", true);
    let (second, _) = browser.open_tab("https://b.test", false);

    assert_eq!(browser.active(), Some(first));
    assert!(slot_of(&browser, second).is_none());
}

#[test]
fn a_single_slot_pool_moves_the_webview_between_tabs() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);

    assert!(
        slot_of(&browser, ids[0]).is_none(),
        "the first tab gave up the only slot"
    );
    assert_eq!(slot_of(&browser, ids[1]), Some(SlotId(0)));

    browser.select_tab(ids[0], 0).unwrap();
    assert_eq!(slot_of(&browser, ids[0]), Some(SlotId(0)));
    assert!(slot_of(&browser, ids[1]).is_none());
}

#[test]
fn selecting_a_discarded_tab_reloads_its_url_into_the_slot() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);

    let effects = browser.select_tab(ids[0], 0).unwrap();

    assert!(effects.contains(&Effect::EnsureSlot {
        profile: Profile::Default,
        slot: SlotId(0),
        url: "https://a.test".to_string(),
        window: browser.main_window(),
    }));
}

#[test]
fn only_the_active_tab_slot_is_shown() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    let effects = browser.select_tab(ids[0], 0).unwrap();

    let active_slot = slot_of(&browser, ids[0]).unwrap();
    let other_slot = slot_of(&browser, ids[1]).unwrap();

    assert!(effects.contains(&Effect::Show { slot: active_slot }));
    assert!(effects.contains(&Effect::Hide { slot: other_slot }));
}

#[test]
fn a_fixed_tab_keeps_its_webview_while_another_tab_is_active() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();
    browser.set_fixed(ids[0], true).unwrap();
    browser.select_tab(ids[1], 0).unwrap();

    assert!(slot_of(&browser, ids[0]).is_some(), "the fixed tab was evicted");
    assert!(slot_of(&browser, ids[1]).is_some(), "the active tab has no webview");
}

#[test]
fn pinning_grows_the_pool_beyond_the_configured_capacity() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();
    browser.set_fixed(ids[0], true).unwrap();
    browser.select_tab(ids[1], 0).unwrap();

    assert_eq!(browser.state().live_count, 2);
    assert_eq!(browser.capacity(), 1, "the configured capacity is unchanged");
}

#[test]
fn an_internal_page_tab_never_takes_a_slot() {
    let (browser, ids) = browser_with(1, &["haku://settings"]);
    assert!(slot_of(&browser, ids[0]).is_none());
    assert!(browser.tab(ids[0]).unwrap().is_internal());
}

#[test]
fn navigating_from_the_web_to_an_internal_page_frees_the_slot() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    assert!(slot_of(&browser, ids[0]).is_some());

    let effects = browser.navigate(ids[0], "haku://settings").unwrap();

    assert!(slot_of(&browser, ids[0]).is_none());
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Blank { .. })));
}

#[test]
fn closing_the_active_tab_activates_its_right_neighbour() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test", "https://c.test"]);
    browser.select_tab(ids[1], 0).unwrap();
    browser.close_tab(ids[1], HOME).unwrap();

    assert_eq!(browser.active(), Some(ids[2]));
}

#[test]
fn closing_the_rightmost_tab_activates_the_one_before_it() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[1], 0).unwrap();
    browser.close_tab(ids[1], HOME).unwrap();

    assert_eq!(browser.active(), Some(ids[0]));
}

#[test]
fn closing_a_tab_blanks_the_webview_it_was_holding() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.close_tab(ids[0], HOME).unwrap();

    assert!(effects.contains(&Effect::Blank { slot }));
}

#[test]
fn closing_the_only_tab_opens_a_new_one_in_its_place() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);

    browser.close_tab(ids[0], HOME).unwrap();

    assert_eq!(browser.tabs().len(), 1);
    assert_ne!(browser.tabs()[0].id, ids[0]);
    assert_eq!(browser.tabs()[0].url(), HOME);
    assert_eq!(browser.active(), Some(browser.tabs()[0].id));
}

#[test]
fn reopening_a_closed_tab_puts_it_back_where_it_was_with_its_history() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test", "https://c.test"]);
    browser.navigate(ids[1], "https://b.test/next").unwrap();
    browser.close_tab(ids[1], HOME).unwrap();

    let effects = browser.reopen_closed_tab();

    let reopened = browser.tabs()[1].clone();
    assert_eq!(reopened.url(), "https://b.test/next");
    assert_eq!(reopened.history.entries().len(), 2);
    assert_eq!(browser.active(), Some(reopened.id));
    assert!(!ids.contains(&reopened.id), "a reopened tab gets a fresh id");
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::EnsureSlot { url, .. } if url == "https://b.test/next"
    )));
}

#[test]
fn closed_tabs_reopen_most_recent_first() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test", "https://c.test"]);
    browser.close_tab(ids[0], HOME).unwrap();
    browser.close_tab(ids[2], HOME).unwrap();

    browser.reopen_closed_tab();
    browser.reopen_closed_tab();

    let urls: Vec<&str> = browser.tabs().iter().map(Tab::url).collect();
    assert_eq!(urls, ["https://a.test", "https://b.test", "https://c.test"]);
}

#[test]
fn only_the_most_recent_closed_tabs_are_remembered() {
    let mut browser = Browser::new(1);
    for index in 0..=CLOSED_LIMIT {
        let (id, _) = browser.open_tab(format!("https://{index}.test"), true);
        browser.close_tab(id, HOME).unwrap();
    }

    let mut reopened = Vec::new();
    while !browser.reopen_closed_tab().is_empty() {
        reopened.push(browser.tab(browser.active().unwrap()).unwrap().url().to_string());
    }

    assert_eq!(reopened.len(), CLOSED_LIMIT);
    assert_eq!(
        reopened[0],
        format!("https://{CLOSED_LIMIT}.test"),
        "the newest comes back first"
    );
    assert!(
        !reopened.contains(&"https://0.test".to_string()),
        "the oldest is forgotten"
    );
}

#[test]
fn an_untouched_new_tab_is_not_remembered_as_closed() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", HOME]);
    browser.close_tab(ids[0], HOME).unwrap();
    browser.close_tab(ids[1], HOME).unwrap();

    browser.reopen_closed_tab();

    assert_eq!(browser.tab(browser.active().unwrap()).unwrap().url(), "https://a.test");
}

#[test]
fn reopening_with_nothing_closed_changes_nothing() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);

    assert!(browser.reopen_closed_tab().is_empty());
    assert_eq!(browser.tabs().len(), 1);
    assert_eq!(browser.active(), Some(ids[0]));
}

#[test]
fn picking_the_next_and_previous_tab_wraps_around() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test", "https://c.test"]);

    assert_eq!(browser.pick(Pick::Next), Some(ids[0]));
    browser.select_tab(ids[0], 0).unwrap();
    assert_eq!(browser.pick(Pick::Previous), Some(ids[2]));
    assert_eq!(browser.pick(Pick::Next), Some(ids[1]));
}

#[test]
fn picking_by_position_ignores_tabs_that_do_not_exist() {
    let (browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);

    assert_eq!(browser.pick(Pick::Nth(0)), Some(ids[0]));
    assert_eq!(browser.pick(Pick::Nth(5)), None);
    assert_eq!(browser.pick(Pick::Last), Some(ids[1]));
}

#[test]
fn closing_an_unknown_tab_reports_it_rather_than_failing_silently() {
    let (mut browser, _) = browser_with(1, &["https://a.test"]);
    assert!(matches!(
        browser.close_tab(TabId(404), HOME),
        Err(HakuError::TabNotFound(_))
    ));
}

#[test]
fn shrinking_capacity_destroys_the_surplus_webviews() {
    let (mut browser, ids) = browser_with(3, &["https://a.test", "https://b.test", "https://c.test"]);
    browser.select_tab(ids[0], 0).unwrap();
    browser.select_tab(ids[1], 0).unwrap();
    assert_eq!(browser.state().live_count, 3);

    let effects = browser.set_capacity(1);

    assert!(effects.iter().any(|effect| matches!(effect, Effect::Destroy { .. })));
    assert_eq!(browser.state().live_count, 1);
}

fn commit(url: &str, kind: NavigationKind) -> Commit {
    Commit {
        url: url.to_string(),
        kind,
    }
}

/// A tab whose first load has already been observed, so later commits are the
/// page's own navigation rather than the one Haku asked for.
fn loaded(url: &str) -> (Browser, TabId, SlotId) {
    let (mut browser, ids) = browser_with(1, &[url]);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.report_page(slot, &[commit(url, NavigationKind::Push)], None);
    (browser, ids[0], slot)
}

#[test]
fn the_first_commit_after_haku_navigates_is_that_navigation_even_when_redirected() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    browser.report_page(
        slot,
        &[commit("https://a.test/final", NavigationKind::Push)],
        Some("Final".into()),
    );

    let tab = browser.tab(ids[0]).unwrap();
    assert_eq!(tab.history.entries().len(), 1);
    assert_eq!(tab.url(), "https://a.test/final");
    assert_eq!(tab.history.current().title, "Final");
}

#[test]
fn a_reported_page_supplies_a_favicon_from_the_site_root() {
    let (browser, id, _) = loaded("https://a.test/deep/page");

    let favicon = browser.tab(id).unwrap().history.current().favicon.clone();
    assert_eq!(favicon.as_deref(), Some("https://a.test/favicon.ico"));
}

#[test]
fn a_link_followed_inside_the_page_adds_a_history_entry() {
    let (mut browser, id, slot) = loaded("https://a.test");

    browser.report_page(slot, &[commit("https://a.test/post", NavigationKind::Push)], None);

    let tab = browser.tab(id).unwrap();
    assert_eq!(tab.history.entries().len(), 2);
    assert_eq!(tab.url(), "https://a.test/post");
    assert!(tab.history.can_go_back());
}

#[test]
fn a_single_page_app_route_change_is_what_the_tab_returns_to() {
    // The reported defect: a route pushed by the page itself was never seen, so
    // leaving the tab and coming back reloaded an older URL.
    let (mut browser, ids) = browser_with(1, &["https://spa.test", "https://other.test"]);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.report_page(slot, &[commit("https://spa.test", NavigationKind::Push)], None);
    browser.report_page(
        slot,
        &[commit("https://spa.test/thread/42", NavigationKind::Push)],
        None,
    );

    browser.select_tab(ids[1], 1).unwrap();
    let effects = browser.select_tab(ids[0], 2).unwrap();

    assert!(effects.contains(&Effect::EnsureSlot {
        profile: Profile::Default,
        slot,
        url: "https://spa.test/thread/42".to_string(),
        window: browser.main_window(),
    }));
}

#[test]
fn a_replaced_route_updates_the_entry_without_growing_history() {
    let (mut browser, id, slot) = loaded("https://a.test");

    browser.report_page(slot, &[commit("https://a.test/?page=2", NavigationKind::Replace)], None);

    let tab = browser.tab(id).unwrap();
    assert_eq!(tab.history.entries().len(), 1);
    assert_eq!(tab.url(), "https://a.test/?page=2");
}

#[test]
fn going_back_inside_the_page_moves_the_tab_back_instead_of_adding_an_entry() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.report_page(slot, &[commit("https://a.test/b", NavigationKind::Push)], None);

    browser.report_page(slot, &[commit("https://a.test", NavigationKind::Traverse)], None);

    let tab = browser.tab(id).unwrap();
    assert_eq!(tab.history.entries().len(), 2);
    assert_eq!(tab.history.index(), 0);
    assert!(tab.history.can_go_forward());
}

#[test]
fn commits_are_applied_in_the_order_the_page_made_them() {
    let (mut browser, id, slot) = loaded("https://a.test");

    browser.report_page(
        slot,
        &[
            commit("https://a.test/one", NavigationKind::Push),
            commit("https://a.test/one?tab=2", NavigationKind::Replace),
            commit("https://a.test/two", NavigationKind::Push),
        ],
        None,
    );

    let tab = browser.tab(id).unwrap();
    let urls: Vec<&str> = tab.history.entries().iter().map(|visit| visit.url.as_str()).collect();
    assert_eq!(
        urls,
        ["https://a.test", "https://a.test/one?tab=2", "https://a.test/two"]
    );
}

#[test]
fn a_title_reported_with_commits_belongs_to_the_last_of_them() {
    // A single-page app pushes its route first and retitles the document after;
    // the new title must not land on the entry the page just left.
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.report_page(slot, &[], Some("Home".into()));

    browser.report_page(
        slot,
        &[commit("https://a.test/post", NavigationKind::Push)],
        Some("Post".into()),
    );

    let entries = browser.tab(id).unwrap().history.entries().to_vec();
    assert_eq!(entries[0].title, "Home");
    assert_eq!(entries[1].title, "Post");
}

#[test]
fn a_pushed_route_keeps_the_page_title_until_the_page_changes_it() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.report_page(slot, &[], Some("Home".into()));

    browser.report_page(slot, &[commit("https://a.test/filter", NavigationKind::Push)], None);

    assert_eq!(browser.tab(id).unwrap().history.current().title, "Home");
}

#[test]
fn the_blank_page_a_parked_slot_shows_is_never_recorded() {
    let (mut browser, id, slot) = loaded("https://a.test");

    assert!(browser
        .report_page(slot, &[commit(BLANK_URL, NavigationKind::Push)], None)
        .is_none());
    assert_eq!(browser.tab(id).unwrap().url(), "https://a.test");
}

#[test]
fn going_back_in_haku_awaits_the_reload_instead_of_pushing_it() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.report_page(slot, &[commit("https://a.test/b", NavigationKind::Push)], None);

    browser.go_back(id).unwrap();
    // The webview reports Haku's own navigation back to the first page as a
    // fresh load; it is the entry already on the stack, not a new one.
    browser.report_page(slot, &[commit("https://a.test", NavigationKind::Push)], None);

    let tab = browser.tab(id).unwrap();
    assert_eq!(tab.history.entries().len(), 2);
    assert_eq!(tab.history.index(), 0);
}

#[test]
fn a_native_back_or_forward_is_resolved_against_the_tab_history() {
    // A webview's own history also holds pages from other tabs that used the
    // same slot, so its back button cannot be trusted to stay inside this tab.
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.report_page(slot, &[commit("https://a.test/b", NavigationKind::Push)], None);

    assert_eq!(
        browser.traversal(slot, "https://another-tab.test"),
        Some((id, Direction::Back))
    );

    browser.go_back(id).unwrap();
    assert_eq!(
        browser.traversal(slot, "https://a.test/b"),
        Some((id, Direction::Forward))
    );
}

#[test]
fn a_favicon_is_only_derived_for_web_pages() {
    assert_eq!(favicon_for("haku://settings"), None);
    assert_eq!(
        favicon_for("https://a.test/x"),
        Some("https://a.test/favicon.ico".to_string())
    );
    assert_eq!(
        favicon_for("http://a.test"),
        Some("http://a.test/favicon.ico".to_string())
    );
}

#[test]
fn going_back_returns_the_tab_to_the_previous_url() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    browser.navigate(ids[0], "https://b.test").unwrap();

    browser.go_back(ids[0]).unwrap();

    assert_eq!(browser.tab(ids[0]).unwrap().url(), "https://a.test");
}

#[test]
fn going_back_at_the_start_of_history_does_nothing() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let effects = browser.go_back(ids[0]).unwrap();

    assert!(effects.is_empty());
}

#[test]
fn reordering_moves_a_tab_to_the_requested_position() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test", "https://c.test"]);
    browser.reorder_tab(ids[2], 0).unwrap();

    let order: Vec<TabId> = browser.tabs().iter().map(|tab| tab.id).collect();
    assert_eq!(order, vec![ids[2], ids[0], ids[1]]);
}

#[test]
fn reordering_past_the_end_clamps_to_the_last_position() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    browser.reorder_tab(ids[0], 99).unwrap();

    let order: Vec<TabId> = browser.tabs().iter().map(|tab| tab.id).collect();
    assert_eq!(order, vec![ids[1], ids[0]]);
}

#[test]
fn an_address_with_a_scheme_is_used_as_typed() {
    assert_eq!(resolve_target("https://a.test/x", SEARCH), "https://a.test/x");
    assert_eq!(resolve_target("haku://settings", SEARCH), "haku://settings");
    assert_eq!(
        resolve_target("chrome-extension://abc/popup.html", SEARCH),
        "chrome-extension://abc/popup.html"
    );
}

#[test]
fn a_bare_host_gets_an_https_scheme() {
    assert_eq!(resolve_target("example.com", SEARCH), "https://example.com");
}

#[test]
fn words_are_searched_rather_than_treated_as_a_host() {
    assert_eq!(resolve_target("how to fly", SEARCH), format!("{SEARCH}how+to+fly"));
}

#[test]
fn a_dotted_phrase_containing_spaces_is_a_search_not_a_host() {
    assert_eq!(
        resolve_target("what is node.js", SEARCH),
        format!("{SEARCH}what+is+node.js")
    );
}

#[test]
fn reserved_characters_in_a_search_are_percent_encoded() {
    assert_eq!(resolve_target("a&b=c", SEARCH), format!("{SEARCH}a%26b%3Dc"));
}

#[test]
fn a_search_goes_where_the_search_url_marks_it() {
    assert_eq!(
        resolve_target("how to fly", "https://search.test/?q=%s&lang=en"),
        "https://search.test/?q=how+to+fly&lang=en"
    );
}

#[test]
fn an_empty_address_resolves_to_nothing() {
    assert_eq!(resolve_target("   ", SEARCH), "");
}

fn dialog(id: u64, kind: DialogKind) -> PageDialog {
    PageDialog {
        id: DialogId(id),
        kind,
        message: "Are you sure?".into(),
        default_text: String::new(),
        url: "https://a.test".into(),
    }
}

fn answers(effects: &[Effect]) -> Vec<(DialogId, DialogAnswer)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::AnswerDialog { id, answer } => Some((*id, answer.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_page_dialog_waits_on_its_tab_until_answered() {
    let (mut browser, id, slot) = loaded("https://a.test");

    let effects = browser.open_dialog(slot, dialog(1, DialogKind::Confirm));

    assert!(answers(&effects).is_empty());
    assert_eq!(
        browser.tab(id).unwrap().dialog.as_ref().map(|open| open.id),
        Some(DialogId(1))
    );
}

#[test]
fn answering_a_dialog_clears_it_and_passes_the_answer_to_the_page() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.open_dialog(slot, dialog(1, DialogKind::Prompt));

    let answer = DialogAnswer::Accept {
        text: Some("typed".into()),
    };
    let effects = browser.answer_dialog(id, DialogId(1), answer.clone()).unwrap();

    assert_eq!(answers(&effects), vec![(DialogId(1), answer)]);
    assert!(browser.tab(id).unwrap().dialog.is_none());
}

#[test]
fn a_late_answer_to_a_dialog_that_is_gone_does_nothing() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.open_dialog(slot, dialog(2, DialogKind::Alert));

    let effects = browser.answer_dialog(id, DialogId(1), DialogAnswer::Dismiss).unwrap();

    assert!(effects.is_empty());
    assert!(browser.tab(id).unwrap().dialog.is_some());
}

#[test]
fn a_tab_that_loses_its_webview_dismisses_the_dialog_first() {
    // With one webview, switching tabs reuses the page's slot. A page paused on
    // a dialog has to be released before the slot can be navigated elsewhere.
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    let slot = slot_of(&browser, ids[1]).unwrap();
    browser.open_dialog(slot, dialog(1, DialogKind::Confirm));

    let effects = browser.select_tab(ids[0], 0).unwrap();

    let answered = effects
        .iter()
        .position(|effect| matches!(effect, Effect::AnswerDialog { .. }));
    let navigated = effects
        .iter()
        .position(|effect| matches!(effect, Effect::EnsureSlot { .. }));
    assert!(
        answered.unwrap() < navigated.unwrap(),
        "the dialog is answered before the slot moves on"
    );
    assert!(browser.tab(ids[1]).unwrap().dialog.is_none());
}

#[test]
fn closing_a_tab_dismisses_its_dialog() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.open_dialog(slot, dialog(1, DialogKind::Alert));

    let effects = browser.close_tab(id, HOME).unwrap();

    assert_eq!(answers(&effects), vec![(DialogId(1), DialogAnswer::Dismiss)]);
}

#[test]
fn navigating_away_from_a_page_on_a_dialog_dismisses_it() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.open_dialog(slot, dialog(1, DialogKind::Confirm));

    let effects = browser.navigate(id, "https://b.test").unwrap();

    assert_eq!(answers(&effects), vec![(DialogId(1), DialogAnswer::Dismiss)]);
}

#[test]
fn leaving_a_page_haku_is_navigating_does_not_ask_the_user() {
    // Haku navigates a slot when it switches tabs or goes back. The old page's
    // "Leave site?" would otherwise be shown on whichever tab now owns the slot.
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.navigate(id, "https://b.test").unwrap();

    let effects = browser.open_dialog(slot, dialog(1, DialogKind::BeforeUnload));

    assert_eq!(
        answers(&effects),
        vec![(DialogId(1), DialogAnswer::Accept { text: None })]
    );
    assert!(browser.tab(id).unwrap().dialog.is_none());
}

#[test]
fn a_dialog_from_a_slot_no_tab_owns_is_answered_at_once() {
    let mut browser = Browser::new(1);

    let alert = browser.open_dialog(SlotId(0), dialog(1, DialogKind::Alert));
    let leave = browser.open_dialog(SlotId(0), dialog(2, DialogKind::BeforeUnload));

    assert_eq!(answers(&alert), vec![(DialogId(1), DialogAnswer::Dismiss)]);
    assert_eq!(
        answers(&leave),
        vec![(DialogId(2), DialogAnswer::Accept { text: None })]
    );
}

// -- what counts as a visit ---------------------------------------------

fn visited(report: Option<PageReport>) -> bool {
    report.expect("the slot has an occupant").visited
}

#[test]
fn a_page_haku_opened_is_a_visit_when_it_arrives() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    assert!(visited(browser.report_page(
        slot,
        &[commit("https://a.test", NavigationKind::Push)],
        None
    )));
}

#[test]
fn a_discarded_tab_reloading_its_page_is_not_a_visit() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    let slot = SlotId(0);
    browser.report_page(slot, &[commit("https://b.test", NavigationKind::Push)], None);
    browser.select_tab(ids[0], 0).unwrap();
    // The first tab's page never arrived before it was discarded, so this is
    // its first load and still a visit.
    assert!(visited(browser.report_page(
        slot,
        &[commit("https://a.test", NavigationKind::Push)],
        None
    )));

    browser.select_tab(ids[1], 0).unwrap();

    assert!(!visited(browser.report_page(
        slot,
        &[commit("https://b.test", NavigationKind::Push)],
        None
    )));
}

#[test]
fn a_restored_session_reloading_its_pages_is_not_a_visit() {
    let mut browser = Browser::restored(1, [(vec![("https://a.test".to_string(), false)], Some(0), vec![])]);
    browser.layout_ready(browser.main_window());
    browser.reconcile();

    assert!(!visited(browser.report_page(
        SlotId(0),
        &[commit("https://a.test", NavigationKind::Push)],
        None
    )));
}

#[test]
fn navigating_a_tab_is_a_visit_when_the_page_arrives() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.navigate(id, "https://b.test").unwrap();

    assert!(visited(browser.report_page(
        slot,
        &[commit("https://b.test", NavigationKind::Push)],
        None
    )));
}

#[test]
fn following_a_link_or_pushing_a_route_is_a_visit() {
    let (mut browser, _, slot) = loaded("https://a.test");

    assert!(visited(browser.report_page(
        slot,
        &[commit("https://a.test/post", NavigationKind::Push)],
        None
    )));
}

#[test]
fn a_replaced_route_or_a_retitle_only_refines_the_current_visit() {
    let (mut browser, _, slot) = loaded("https://a.test");

    assert!(!visited(browser.report_page(
        slot,
        &[commit("https://a.test/?q=1", NavigationKind::Replace)],
        None
    )));
    assert!(!visited(browser.report_page(slot, &[], Some("Renamed".into()))));
}

// -- optimization policies -------------------------------------------------

/// Three tabs in a three-slot pool, with the last one opened active.
fn optimized(freeze: Policy, discard: Policy) -> (Browser, Vec<TabId>) {
    let mut browser = Browser::new(3).with_policies(freeze, discard);
    let ids = ["https://a.test", "https://b.test", "https://c.test"]
        .iter()
        .map(|url| browser.open_tab(*url, true).0)
        .collect();
    (browser, ids)
}

fn presence(browser: &Browser, id: TabId) -> TabPresence {
    browser.tab(id).unwrap().presence
}

fn position(effects: &[Effect], wanted: &Effect) -> usize {
    effects
        .iter()
        .position(|effect| effect == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} missing from {effects:?}"))
}

#[test]
fn a_tab_left_in_the_background_is_frozen_after_it_is_hidden() {
    let mut browser = Browser::new(3).with_policies(Policy::Smart, Policy::Never);
    let (first, _) = browser.open_tab("https://a.test", true);
    let slot = slot_of(&browser, first).unwrap();

    let (_, effects) = browser.open_tab("https://b.test", true);

    assert_eq!(presence(&browser, first), TabPresence::Frozen { slot });
    assert!(position(&effects, &Effect::Hide { slot }) < position(&effects, &Effect::Freeze { slot }));
}

#[test]
fn returning_to_a_frozen_tab_resumes_it_without_reloading() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Never);
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.select_tab(ids[0], 0).unwrap();

    assert_eq!(presence(&browser, ids[0]), TabPresence::Live { slot });
    assert!(position(&effects, &Effect::Resume { slot }) < position(&effects, &Effect::Show { slot }));
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::EnsureSlot { .. } | Effect::Reload { .. })));
}

#[test]
fn freezing_never_leaves_background_tabs_running() {
    let (browser, ids) = optimized(Policy::Never, Policy::Never);
    assert!(matches!(presence(&browser, ids[0]), TabPresence::Live { .. }));
}

#[test]
fn turning_freezing_off_resumes_the_tabs_it_froze() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Never);
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.set_optimization(3, Policy::Never, Policy::Never);

    assert!(effects.contains(&Effect::Resume { slot }));
    assert_eq!(presence(&browser, ids[0]), TabPresence::Live { slot });
}

#[test]
fn a_smart_policy_leaves_a_tab_playing_audio_running() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Never);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.report_audio(slot, true, 0);

    browser.select_tab(ids[1], 0).unwrap();

    assert_eq!(presence(&browser, ids[0]), TabPresence::Live { slot });
}

#[test]
fn a_background_tab_that_falls_silent_is_frozen() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Never);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.report_audio(slot, true, 0);
    browser.select_tab(ids[1], 0).unwrap();

    let effects = browser.report_audio(slot, false, 0);

    assert!(effects.contains(&Effect::Freeze { slot }));
}

#[test]
fn freezing_always_freezes_a_tab_playing_audio_too() {
    let (mut browser, ids) = optimized(Policy::Always, Policy::Never);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.report_audio(slot, true, 0);

    browser.select_tab(ids[1], 0).unwrap();

    assert_eq!(presence(&browser, ids[0]), TabPresence::Frozen { slot });
}

#[test]
fn a_fixed_tab_is_never_frozen() {
    let (mut browser, ids) = optimized(Policy::Always, Policy::Never);
    browser.set_fixed(ids[0], true).unwrap();
    browser.select_tab(ids[0], 0).unwrap();

    browser.select_tab(ids[1], 0).unwrap();

    assert!(matches!(presence(&browser, ids[0]), TabPresence::Live { .. }));
}

#[test]
fn discarding_always_releases_a_tab_as_soon_as_it_is_left() {
    let (browser, ids) = optimized(Policy::Smart, Policy::Always);

    assert_eq!(presence(&browser, ids[0]), TabPresence::Discarded);
    assert_eq!(presence(&browser, ids[1]), TabPresence::Discarded);
    assert!(matches!(presence(&browser, ids[2]), TabPresence::Live { .. }));
}

#[test]
fn discarding_always_still_spares_a_fixed_tab() {
    let (mut browser, ids) = optimized(Policy::Never, Policy::Always);
    browser.select_tab(ids[0], 0).unwrap();
    browser.set_fixed(ids[0], true).unwrap();

    browser.select_tab(ids[1], 0).unwrap();

    assert!(matches!(presence(&browser, ids[0]), TabPresence::Live { .. }));
}

#[test]
fn a_frozen_tab_is_resumed_before_its_slot_is_handed_to_another_tab() {
    let mut browser = Browser::new(2).with_policies(Policy::Smart, Policy::Never);
    let (first, _) = browser.open_tab("https://a.test", true);
    browser.open_tab("https://b.test", true);
    let slot = slot_of(&browser, first).unwrap();

    // A third tab in a two-slot pool evicts the coldest, the frozen first one.
    let (_, effects) = browser.open_tab("https://c.test", true);

    let ensure = effects
        .iter()
        .position(|effect| matches!(effect, Effect::EnsureSlot { slot: target, .. } if *target == slot))
        .expect("the slot is navigated to the new tab");
    assert!(position(&effects, &Effect::Resume { slot }) < ensure);
    assert_eq!(presence(&browser, first), TabPresence::Discarded);
}

#[test]
fn closing_a_frozen_tab_resumes_its_slot_before_parking_it() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Never);
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.close_tab(ids[0], HOME).unwrap();

    assert!(position(&effects, &Effect::Resume { slot }) < position(&effects, &Effect::Blank { slot }));
}

/// A one-slot browser whose first tab plays audio while the second is shown.
fn music_in_background(freeze: Policy, discard: Policy) -> (Browser, Vec<TabId>, SlotId) {
    let mut browser = Browser::new(1).with_policies(freeze, discard);
    let (music, _) = browser.open_tab("https://music.test", true);
    let slot = slot_of(&browser, music).unwrap();
    browser.report_audio(slot, true, 0);
    let (other, _) = browser.open_tab("https://b.test", true);
    (browser, vec![music, other], slot)
}

#[test]
fn music_survives_a_tab_switch_at_a_capacity_of_one() {
    let (browser, ids, slot) = music_in_background(Policy::Smart, Policy::Smart);

    assert_eq!(presence(&browser, ids[0]), TabPresence::Live { slot });
    assert!(
        slot_of(&browser, ids[1]).is_some(),
        "the visible tab got a webview of its own"
    );
    assert_eq!(browser.capacity(), 1, "the configured capacity is unchanged");
}

#[test]
fn an_audible_background_tab_is_not_evicted_for_another_tab() {
    let (mut browser, ids, slot) = music_in_background(Policy::Smart, Policy::Smart);

    let (third, _) = browser.open_tab("https://c.test", true);

    assert_eq!(presence(&browser, ids[0]), TabPresence::Live { slot });
    assert!(slot_of(&browser, third).is_some());
}

#[test]
fn a_tab_that_falls_silent_gives_up_its_reservation() {
    let (mut browser, ids, slot) = music_in_background(Policy::Smart, Policy::Smart);
    browser.report_audio(slot, false, 0);

    browser.open_tab("https://c.test", true);

    assert_eq!(presence(&browser, ids[0]), TabPresence::Discarded);
}

#[test]
fn discarding_always_does_not_honour_audio() {
    let (browser, ids, _) = music_in_background(Policy::Smart, Policy::Always);
    assert_eq!(presence(&browser, ids[0]), TabPresence::Discarded);
}

#[test]
fn a_discarded_tab_no_longer_counts_as_playing() {
    let (browser, ids, _) = music_in_background(Policy::Smart, Policy::Always);
    assert!(!browser.tab(ids[0]).unwrap().audible);
}

#[test]
fn eviction_takes_the_tab_shown_least_recently() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();

    browser.open_tab("https://c.test", true);

    assert!(slot_of(&browser, ids[0]).is_some(), "the tab shown last stays");
    assert_eq!(presence(&browser, ids[1]), TabPresence::Discarded);
}

#[test]
fn shrinking_capacity_keeps_the_visible_tab_where_it_is() {
    let (mut browser, ids) = browser_with(3, &["https://a.test", "https://b.test", "https://c.test"]);
    browser.select_tab(ids[1], 0).unwrap();
    let slot = slot_of(&browser, ids[1]).unwrap();

    let effects = browser.set_capacity(1);

    assert!(!effects.contains(&Effect::Destroy { slot }));
    assert_eq!(slot_of(&browser, ids[1]), Some(slot));
}

#[test]
fn shrinking_capacity_spares_an_audible_tab() {
    let mut browser = Browser::new(3).with_policies(Policy::Never, Policy::Never);
    let (music, _) = browser.open_tab("https://music.test", true);
    let slot = slot_of(&browser, music).unwrap();
    browser.report_audio(slot, true, 0);
    browser.open_tab("https://b.test", true);
    browser.open_tab("https://c.test", true);

    browser.set_capacity(1);

    assert_eq!(presence(&browser, music), TabPresence::Live { slot });
    assert_eq!(browser.state().live_count, 2);
}

#[test]
fn shrinking_capacity_destroys_a_parked_webview_before_a_loaded_one() {
    let (mut browser, ids) = browser_with(3, &["https://a.test", "https://b.test", "https://c.test"]);
    let parked = slot_of(&browser, ids[1]).unwrap();
    browser.close_tab(ids[1], HOME).unwrap();

    let effects = browser.set_capacity(2);

    let destroyed: Vec<&Effect> = effects
        .iter()
        .filter(|effect| matches!(effect, Effect::Destroy { .. }))
        .collect();
    assert_eq!(destroyed, vec![&Effect::Destroy { slot: parked }]);
}

fn reading(url: &str) -> PageState {
    PageState {
        url: url.to_string(),
        ..PageState::default()
    }
}

fn scrolled(url: &str, y: f64, draft: Option<&str>) -> PageState {
    PageState {
        scroll: Scroll { x: 0.0, y },
        draft: draft.map(str::to_string),
        ..reading(url)
    }
}

#[test]
fn a_page_is_read_before_it_is_frozen() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Never);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.select_tab(ids[1], 0).unwrap();

    let leave = Effect::Leave {
        slot,
        tab: ids[0],
        capture: true,
    };
    assert!(position(&effects, &leave) < position(&effects, &Effect::Freeze { slot }));
}

#[test]
fn a_page_is_read_before_it_is_parked() {
    let (mut browser, ids) = optimized(Policy::Never, Policy::Always);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.select_tab(ids[1], 0).unwrap();

    let leave = Effect::Leave {
        slot,
        tab: ids[0],
        capture: true,
    };
    assert!(position(&effects, &leave) < position(&effects, &Effect::Blank { slot }));
}

#[test]
fn an_evicted_page_is_read_before_another_tab_takes_its_slot() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    let (_, effects) = browser.open_tab("https://b.test", true);

    let leave = Effect::Leave {
        slot,
        tab: ids[0],
        capture: true,
    };
    let ensure = Effect::EnsureSlot {
        profile: Profile::Default,
        slot,
        url: "https://b.test".into(),
        window: browser.main_window(),
    };
    assert!(position(&effects, &leave) < position(&effects, &ensure));
}

#[test]
fn a_frozen_page_is_not_read_again_when_it_is_evicted() {
    let mut browser = Browser::new(2).with_policies(Policy::Smart, Policy::Never);
    let (first, _) = browser.open_tab("https://a.test", true);
    browser.open_tab("https://b.test", true);

    let (_, effects) = browser.open_tab("https://c.test", true);

    assert_eq!(presence(&browser, first), TabPresence::Discarded);
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::Leave { tab, .. } if *tab == first)));
}

#[test]
fn a_closing_tab_is_not_read() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);

    let effects = browser.close_tab(ids[0], HOME).unwrap();

    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Leave { .. })));
}

#[test]
fn a_reading_keeps_the_scroll_and_draft_of_the_page() {
    let (mut browser, id, _) = loaded("https://a.test");

    browser.report_state(id, Some(scrolled("https://a.test", 480.0, Some("[]"))));

    let tab = browser.tab(id).unwrap();
    assert_eq!(tab.scroll, Scroll { x: 0.0, y: 480.0 });
    assert_eq!(tab.draft.as_deref(), Some("[]"));
}

#[test]
fn a_reading_taken_on_another_url_is_not_applied() {
    let (mut browser, id, _) = loaded("https://a.test");

    browser.report_state(id, Some(scrolled("https://elsewhere.test", 480.0, Some("[]"))));

    let tab = browser.tab(id).unwrap();
    assert_eq!(tab.scroll, Scroll::default());
    assert!(tab.draft.is_none());
}

#[test]
fn a_draft_over_the_limit_is_dropped() {
    let (mut browser, id, _) = loaded("https://a.test");
    let huge = "x".repeat(crate::model::page_state::DRAFT_LIMIT + 1);

    browser.report_state(id, Some(scrolled("https://a.test", 0.0, Some(&huge))));

    assert!(browser.tab(id).unwrap().draft.is_none());
}

#[test]
fn a_page_that_cannot_be_read_would_lose_state() {
    let (mut browser, id, _) = loaded("https://a.test");

    browser.report_state(id, None);

    assert_eq!(browser.tab(id).unwrap().page.loss(), Loss::State);
}

#[test]
fn a_pushed_route_clears_the_scroll_and_draft() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.report_state(id, Some(scrolled("https://a.test", 480.0, Some("[]"))));

    browser.report_page(slot, &[commit("https://a.test/next", NavigationKind::Push)], None);

    let tab = browser.tab(id).unwrap();
    assert_eq!(tab.scroll, Scroll::default());
    assert!(tab.draft.is_none());
}

#[test]
fn a_discarded_tab_gets_its_scroll_and_draft_back_in_any_slot() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    browser.report_state(ids[1], Some(scrolled("https://b.test", 480.0, Some("[]"))));
    browser.select_tab(ids[0], 0).unwrap();

    let effects = browser.select_tab(ids[1], 0).unwrap();

    let slot = slot_of(&browser, ids[1]).unwrap();
    assert!(effects.contains(&Effect::RestoreState {
        slot,
        url: "https://b.test".into(),
        scroll: Scroll { x: 0.0, y: 480.0 },
        height: None,
        draft: Some("[]".into()),
    }));
}

#[test]
fn the_draft_never_reaches_the_interface() {
    let (mut browser, id, _) = loaded("https://a.test");
    browser.report_state(id, Some(scrolled("https://a.test", 0.0, Some("typed text"))));

    let json = serde_json::to_string(&browser.state()).unwrap();

    assert!(!json.contains("typed text"));
}

#[test]
fn a_page_loaded_by_a_form_submission_is_work() {
    let (mut browser, id, slot) = loaded("https://a.test");

    browser.report_navigation(slot, true);
    browser.report_page(slot, &[commit("https://a.test/sent", NavigationKind::Push)], None);

    assert_eq!(browser.tab(id).unwrap().page.loss(), Loss::Work);
}

#[test]
fn the_next_navigation_clears_a_form_result() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.report_navigation(slot, true);

    browser.report_navigation(slot, false);

    assert_eq!(browser.tab(id).unwrap().page.loss(), Loss::None);
}

/// A one-slot browser whose first tab is capturing while the second is shown.
fn capturing_in_background(freeze: Policy) -> (Browser, Vec<TabId>, SlotId) {
    let mut browser = Browser::new(1).with_policies(freeze, Policy::Smart);
    let (call, _) = browser.open_tab("https://call.test", true);
    let slot = slot_of(&browser, call).unwrap();
    browser.report_state(
        call,
        Some(PageState {
            capturing: true,
            ..reading("https://call.test")
        }),
    );
    let (other, _) = browser.open_tab("https://b.test", true);
    (browser, vec![call, other], slot)
}

#[test]
fn a_capturing_tab_keeps_running_in_the_background() {
    let (browser, ids, slot) = capturing_in_background(Policy::Smart);

    assert_eq!(presence(&browser, ids[0]), TabPresence::Live { slot });
    assert!(slot_of(&browser, ids[1]).is_some());
}

#[test]
fn freezing_always_does_not_honour_capture() {
    let (browser, ids, _) = capturing_in_background(Policy::Always);
    assert_eq!(presence(&browser, ids[0]), TabPresence::Discarded);
}

#[test]
fn a_page_found_capturing_as_it_was_frozen_is_resumed() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Never);
    let slot = slot_of(&browser, ids[0]).unwrap();
    assert_eq!(presence(&browser, ids[0]), TabPresence::Frozen { slot });

    let effects = browser.report_state(
        ids[0],
        Some(PageState {
            capturing: true,
            ..reading("https://a.test")
        }),
    );

    assert!(effects.contains(&Effect::Resume { slot }));
}

#[test]
fn a_reading_that_changes_nothing_produces_no_effects() {
    let (mut browser, id, _) = loaded("https://a.test");

    let effects = browser.report_state(id, Some(scrolled("https://a.test", 10.0, None)));

    assert!(effects.is_empty());
}

// -- the rule ---------------------------------------------------------------

const MB: u64 = 1024 * 1024;

/// Four tabs in a four-slot pool under smart discarding. The first three were
/// left in order, the fourth is visible, and none has been read yet.
fn smart(budget: u64) -> (Browser, Vec<TabId>) {
    let mut browser = Browser::new(4)
        .with_policies(Policy::Smart, Policy::Smart)
        .with_keeping(budget, Vec::new());
    let ids: Vec<TabId> = ["https://a.test", "https://b.test", "https://c.test", "https://d.test"]
        .iter()
        .map(|url| browser.open_tab(*url, true).0)
        .collect();
    for (offset, id) in ids.iter().enumerate() {
        browser.select_tab(*id, 1_000 + offset as u64).unwrap();
    }
    (browser, ids)
}

/// When the last background tab of [`smart`] was left.
const LEFT: u64 = 1_003;

fn holding(url: &str, loss: Loss) -> PageState {
    match loss {
        Loss::None => reading(url),
        Loss::State => PageState {
            interactions: INTERACTION_THRESHOLD,
            ..reading(url)
        },
        Loss::Work => PageState {
            unsaved: true,
            ..reading(url)
        },
    }
}

/// Records what each tab would lose.
fn set_losses(browser: &mut Browser, ids: &[TabId], losses: &[Loss]) {
    for (id, loss) in ids.iter().zip(losses) {
        let url = browser.tab(*id).unwrap().url().to_string();
        browser.report_state(*id, Some(holding(&url, *loss)));
    }
}

/// The same measured memory for every slot.
fn every_slot(browser: &Browser, bytes: u64) -> HashMap<SlotId, u64> {
    browser.slot_ids().into_iter().map(|slot| (slot, bytes)).collect()
}

fn discarded(browser: &Browser, id: TabId) -> bool {
    presence(browser, id) == TabPresence::Discarded
}

#[test]
fn a_tab_with_nothing_to_lose_is_discarded_once_grace_ends_and_not_before() {
    let (mut browser, ids) = smart(0);
    let slot = slot_of(&browser, ids[2]).unwrap();

    browser.tick(LEFT + GRACE_MS - 1, Pressure::Normal, HashMap::new());
    assert!(!discarded(&browser, ids[2]), "still in grace");

    let effects = browser.tick(LEFT + GRACE_MS, Pressure::Normal, HashMap::new());
    assert!(discarded(&browser, ids[2]));
    // Frozen while in grace, so it was read then and is not read again.
    assert!(position(&effects, &Effect::Resume { slot }) < position(&effects, &Effect::Blank { slot }));
    assert!(!discarded(&browser, ids[3]), "the visible tab stays");
}

#[test]
fn a_tab_in_grace_is_kept_frozen() {
    let (browser, ids) = smart(0);
    assert!(matches!(presence(&browser, ids[2]), TabPresence::Frozen { .. }));
}

#[test]
fn a_work_tab_is_kept_past_the_budget_and_a_state_tab_is_not() {
    let (mut browser, ids) = smart(50 * MB);
    set_losses(&mut browser, &ids, &[Loss::Work, Loss::State, Loss::None]);

    let memory = every_slot(&browser, 100 * MB);
    browser.tick(LEFT + GRACE_MS, Pressure::Normal, memory);

    assert!(!discarded(&browser, ids[0]), "work is kept");
    assert!(discarded(&browser, ids[1]), "state does not fit");
    assert!(discarded(&browser, ids[2]), "nothing to lose");
}

#[test]
fn state_tabs_are_kept_most_recent_first_until_the_budget_is_spent() {
    let (mut browser, ids) = smart(250 * MB);
    set_losses(&mut browser, &ids, &[Loss::State, Loss::State, Loss::State]);

    let memory = every_slot(&browser, 100 * MB);
    browser.tick(LEFT + GRACE_MS, Pressure::Normal, memory);

    assert!(discarded(&browser, ids[0]), "the least recent goes");
    assert!(!discarded(&browser, ids[1]));
    assert!(!discarded(&browser, ids[2]));
}

#[test]
fn tabs_kept_outright_spend_the_budget_first() {
    let (mut browser, ids) = smart(150 * MB);
    set_losses(&mut browser, &ids, &[Loss::State, Loss::Work, Loss::None]);

    let memory = every_slot(&browser, 100 * MB);
    browser.tick(LEFT + GRACE_MS, Pressure::Normal, memory);

    assert!(!discarded(&browser, ids[1]));
    assert!(discarded(&browser, ids[0]), "the work tab left no room");
}

#[test]
fn unmeasured_tabs_cost_nothing_against_the_budget() {
    let (mut browser, ids) = smart(0);
    set_losses(&mut browser, &ids, &[Loss::State, Loss::State, Loss::State]);

    browser.tick(LEFT + GRACE_MS, Pressure::Normal, HashMap::new());

    assert!(ids[..3].iter().all(|id| !discarded(&browser, *id)));
}

#[test]
fn a_tab_on_a_kept_site_is_kept_like_work() {
    let (mut browser, ids) = smart(0);
    browser.set_keeping(0, vec!["a.test".into()]);

    let memory = every_slot(&browser, 100 * MB);
    browser.tick(LEFT + GRACE_MS, Pressure::Normal, memory);

    assert!(!discarded(&browser, ids[0]));
    assert!(discarded(&browser, ids[1]));
}

#[test]
fn a_tab_paused_on_a_dialog_is_left_alone() {
    let (mut browser, ids) = smart(0);
    browser.open_dialog(slot_of(&browser, ids[0]).unwrap(), dialog(1, DialogKind::Alert));

    browser.tick(LEFT + GRACE_MS, Pressure::Critical, HashMap::new());

    assert!(!discarded(&browser, ids[0]));
}

#[test]
fn a_tick_that_changes_nothing_produces_no_effects() {
    let (mut browser, _) = smart(0);
    assert!(browser.tick(LEFT + 1, Pressure::Normal, HashMap::new()).is_empty());
}

#[test]
fn tight_pressure_discards_state_inside_grace_but_keeps_work() {
    let (mut browser, ids) = smart(u64::MAX);
    set_losses(&mut browser, &ids, &[Loss::Work, Loss::State, Loss::None]);

    browser.tick(LEFT + 1, Pressure::Tight, HashMap::new());

    assert!(!discarded(&browser, ids[0]));
    assert!(discarded(&browser, ids[1]));
    assert!(discarded(&browser, ids[2]));
    assert!(!discarded(&browser, ids[3]), "the visible tab stays");
}

#[test]
fn critical_pressure_discards_work_too_but_never_a_visible_or_must_run_tab() {
    let (mut browser, ids) = smart(u64::MAX);
    set_losses(&mut browser, &ids, &[Loss::Work, Loss::None, Loss::None]);
    browser.report_audio(slot_of(&browser, ids[1]).unwrap(), true, LEFT);

    browser.tick(LEFT + 1, Pressure::Critical, HashMap::new());

    assert!(discarded(&browser, ids[0]));
    assert!(!discarded(&browser, ids[1]), "music keeps playing");
    assert!(!discarded(&browser, ids[3]));
}

#[test]
fn discarding_never_discards_nothing_even_at_critical() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Never);

    let effects = browser.tick(GRACE_MS * 10, Pressure::Critical, HashMap::new());

    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::Blank { .. } | Effect::Destroy { .. })));
    assert!(slot_of(&browser, ids[0]).is_some());
}

#[test]
fn pressure_read_on_a_tab_switch_acts_at_once() {
    let (mut browser, ids) = smart(u64::MAX);

    browser.set_pressure(Pressure::Tight);
    browser.select_tab(ids[0], LEFT + 1).unwrap();

    assert!(discarded(&browser, ids[3]), "the tab just left has no grace");
}

#[test]
fn a_tab_discarded_under_pressure_is_marked_until_it_is_loaded_again() {
    let (mut browser, ids) = smart(u64::MAX);

    browser.tick(LEFT + 1, Pressure::Tight, HashMap::new());
    assert!(browser.tab(ids[0]).unwrap().relieved);

    browser.select_tab(ids[0], LEFT + 2).unwrap();
    assert!(!browser.tab(ids[0]).unwrap().relieved);
}

#[test]
fn a_tab_discarded_at_normal_pressure_is_not_marked() {
    let (mut browser, ids) = smart(0);
    browser.tick(LEFT + GRACE_MS, Pressure::Normal, HashMap::new());
    assert!(discarded(&browser, ids[0]));
    assert!(!browser.tab(ids[0]).unwrap().relieved);
}

#[test]
fn one_parked_webview_is_kept_as_a_spare_at_normal_pressure() {
    let (mut browser, _) = smart(0);

    let effects = browser.tick(LEFT + GRACE_MS, Pressure::Normal, HashMap::new());

    let destroyed = effects
        .iter()
        .filter(|effect| matches!(effect, Effect::Destroy { .. }))
        .count();
    assert_eq!(destroyed, 2, "three tabs were discarded and one slot is kept");
    assert_eq!(browser.slots().len(), 2);
}

#[test]
fn no_parked_webview_is_kept_under_pressure() {
    let (mut browser, _) = smart(0);
    browser.tick(LEFT + GRACE_MS, Pressure::Normal, HashMap::new());

    browser.tick(LEFT + GRACE_MS + 1, Pressure::Tight, HashMap::new());

    assert_eq!(browser.slots().len(), 1, "only the visible tab's");
}

#[test]
fn eviction_takes_the_lowest_loss_before_the_least_recent() {
    let mut browser = Browser::new(3);
    let (a, _) = browser.open_tab("https://a.test", true);
    let (b, _) = browser.open_tab("https://b.test", true);
    browser.open_tab("https://c.test", true);
    browser.report_state(a, Some(holding("https://a.test", Loss::Work)));

    browser.open_tab("https://d.test", true);

    assert!(!discarded(&browser, a), "the older tab holds work");
    assert!(discarded(&browser, b));
}

#[test]
fn a_work_tab_is_evicted_when_nothing_else_can_be() {
    let mut browser = Browser::new(1);
    let (a, _) = browser.open_tab("https://a.test", true);
    browser.report_state(a, Some(holding("https://a.test", Loss::Work)));

    let (b, _) = browser.open_tab("https://b.test", true);

    assert!(discarded(&browser, a));
    assert!(slot_of(&browser, b).is_some());
}

#[test]
fn freezing_always_lets_smart_discarding_take_an_audible_tab() {
    let mut browser = Browser::new(3).with_policies(Policy::Always, Policy::Smart);
    let (music, _) = browser.open_tab("https://music.test", true);
    let slot = slot_of(&browser, music).unwrap();
    browser.report_audio(slot, true, 0);
    browser.open_tab("https://b.test", true);
    browser.tick(1_000, Pressure::Normal, HashMap::new());

    browser.tick(1_000 + GRACE_MS, Pressure::Normal, HashMap::new());

    assert_eq!(presence(&browser, music), TabPresence::Discarded);
}

#[test]
fn a_tab_that_falls_silent_is_timed_from_that_moment() {
    let (mut browser, ids) = optimized(Policy::Never, Policy::Smart);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.report_audio(slot, true, 0);
    browser.tick(1_000, Pressure::Normal, HashMap::new());
    browser.report_audio(slot, false, 1_000 + GRACE_MS);

    browser.tick(1_000 + GRACE_MS, Pressure::Normal, HashMap::new());

    assert!(slot_of(&browser, ids[0]).is_some());
}

// -- previews ---------------------------------------------------------------

fn leaves(effects: &[Effect], id: TabId) -> Vec<&Effect> {
    effects
        .iter()
        .filter(|effect| matches!(effect, Effect::Leave { tab, .. } if *tab == id))
        .collect()
}

#[test]
fn the_visible_page_is_captured_before_it_is_hidden() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Never);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.select_tab(ids[1], 0).unwrap();

    let leave = Effect::Leave {
        slot,
        tab: ids[0],
        capture: true,
    };
    assert!(position(&effects, &leave) < position(&effects, &Effect::Hide { slot }));
    assert_eq!(
        leaves(&effects, ids[0]).len(),
        1,
        "read once, not again as it is frozen"
    );
}

#[test]
fn a_background_page_is_read_without_a_capture() {
    let mut browser = Browser::new(2);
    let (first, _) = browser.open_tab("https://a.test", true);
    browser.open_tab("https://b.test", true);

    let (_, effects) = browser.open_tab("https://c.test", true);

    assert_eq!(
        leaves(&effects, first),
        vec![&Effect::Leave {
            slot: SlotId(0),
            tab: first,
            capture: false
        }]
    );
}

#[test]
fn staying_on_the_same_tab_captures_nothing() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);

    let effects = browser.navigate(ids[0], "https://a.test/next").unwrap();

    assert!(leaves(&effects, ids[0]).is_empty());
}

#[test]
fn a_tab_loading_into_a_slot_is_restoring_until_its_document_loads() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();
    assert!(browser.tab(ids[0]).unwrap().restoring);

    assert!(!browser.report_loaded(slot, BLANK_URL), "the blank page is not it");
    assert!(browser.tab(ids[0]).unwrap().restoring);

    assert!(browser.report_loaded(slot, "https://a.test/"));
    assert!(!browser.tab(ids[0]).unwrap().restoring);
}

#[test]
fn navigating_a_loaded_tab_is_not_a_restore() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.report_loaded(slot, "https://a.test/");

    browser.navigate(id, "https://a.test/next").unwrap();

    assert!(!browser.tab(id).unwrap().restoring);
}

#[test]
fn a_tab_that_loses_its_slot_stops_restoring() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();

    browser.select_tab(ids[1], 0).unwrap();

    assert!(!browser.tab(ids[0]).unwrap().restoring);
}

#[test]
fn a_restore_carries_the_height_the_page_had_when_it_was_read() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    browser.report_state(
        ids[1],
        Some(PageState {
            height: 9000.0,
            ..scrolled("https://b.test", 480.0, None)
        }),
    );
    browser.select_tab(ids[0], 0).unwrap();

    let effects = browser.select_tab(ids[1], 0).unwrap();

    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::RestoreState { height: Some(height), .. } if *height == 9000.0)));
}

const REQUEST: WindowRequestId = WindowRequestId(7);

fn ids_of(browser: &Browser) -> Vec<TabId> {
    browser.tabs().iter().map(|tab| tab.id).collect()
}

#[test]
fn a_link_opened_from_a_page_becomes_the_active_tab_beside_it() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();

    let (opened, _) = browser.open_from(slot, "https://c.test", Opening::Foreground);

    assert_eq!(ids_of(&browser), vec![ids[0], opened, ids[1]]);
    assert_eq!(browser.active(), Some(opened));
}

#[test]
fn links_opened_for_later_line_up_after_their_opener_in_order() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();

    let (first, _) = browser.open_from(slot, "https://c.test", Opening::Background);
    let (second, _) = browser.open_from(slot, "https://d.test", Opening::Background);

    assert_eq!(ids_of(&browser), vec![ids[0], first, second, ids[1]]);
    assert_eq!(browser.active(), Some(ids[0]));
    assert!(slot_of(&browser, first).is_none(), "loads when selected");
}

#[test]
fn a_popup_opens_in_a_connected_tab_and_its_opener_keeps_running() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let opener = slot_of(&browser, ids[0]).unwrap();

    let (popup, effects) = browser.open_from(opener, "https://login.test", Opening::Connected(REQUEST));

    let slot = slot_of(&browser, popup).unwrap();
    assert_ne!(slot, opener);
    assert!(effects.contains(&Effect::Adopt {
        profile: Profile::Default,
        slot,
        request: REQUEST,
        url: "https://login.test".to_string(),
        window: browser.main_window(),
    }));
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::EnsureSlot { slot: s, .. } if *s == slot)));
    assert_eq!(
        slot_of(&browser, ids[0]),
        Some(opener),
        "the sign-in reports back to it"
    );
    assert_eq!(browser.position(ids[0]), Some(Position::MustRun));
}

#[test]
fn a_connected_tab_never_gets_a_parked_webview() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    browser.close_tab(ids[1], HOME).unwrap();
    let opener = slot_of(&browser, ids[0]).unwrap();
    let parked = browser.slots().iter().find(|slot| slot.occupant.is_none()).unwrap().id;

    let (popup, effects) = browser.open_from(opener, "https://login.test", Opening::Connected(REQUEST));

    assert_ne!(slot_of(&browser, popup), Some(parked));
    assert!(
        effects.contains(&Effect::Destroy { slot: parked }),
        "the pool keeps its size"
    );
    assert_eq!(browser.slots().len(), 2);
}

#[test]
fn closing_a_connected_tab_returns_to_the_page_that_opened_it() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();
    let opener = slot_of(&browser, ids[0]).unwrap();
    let (popup, _) = browser.open_from(opener, "https://login.test", Opening::Connected(REQUEST));

    browser.close_tab(popup, HOME).unwrap();

    assert_eq!(browser.active(), Some(ids[0]));
}

#[test]
fn an_opener_stops_running_once_its_connected_tab_closes() {
    let (mut browser, ids) = browser_with(3, &["https://a.test"]);
    let opener = slot_of(&browser, ids[0]).unwrap();
    let (popup, _) = browser.open_from(opener, "https://login.test", Opening::Connected(REQUEST));
    browser.open_tab("https://b.test", true);
    assert_eq!(browser.position(ids[0]), Some(Position::MustRun));

    browser.close_tab(popup, HOME).unwrap();

    assert_ne!(browser.position(ids[0]), Some(Position::MustRun));
}

#[test]
fn an_opener_stops_running_once_its_connected_tab_loses_its_page() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let opener = slot_of(&browser, ids[0]).unwrap();
    let (popup, _) = browser.open_from(opener, "https://login.test", Opening::Connected(REQUEST));

    browser.open_tab("https://b.test", true);

    assert!(slot_of(&browser, popup).is_none(), "evicted, which cuts the connection");
    assert_ne!(browser.position(ids[0]), Some(Position::MustRun));
}

#[test]
fn discarding_always_spares_an_opener_while_its_connected_tab_is_open() {
    let mut browser = Browser::new(2).with_policies(Policy::Never, Policy::Always);
    let (first, _) = browser.open_tab("https://a.test", true);
    let opener = slot_of(&browser, first).unwrap();

    browser.open_from(opener, "https://login.test", Opening::Connected(REQUEST));

    assert_eq!(slot_of(&browser, first), Some(opener), "the sign-in reports back to it");
}

#[test]
fn the_tab_in_a_slot_is_the_one_it_was_opened_from() {
    let (browser, ids) = browser_with(1, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    assert_eq!(browser.tab_in(slot), Some(ids[0]));
    assert_eq!(browser.tab_in(SlotId(9)), None);
}

// -- windows ---------------------------------------------------------------

fn window_tabs(browser: &Browser, window: WindowId) -> Vec<TabId> {
    browser
        .state_in(window)
        .unwrap()
        .tabs
        .iter()
        .map(|tab| tab.id)
        .collect()
}

const POPUP_PLACEMENT: Placement = Placement {
    position: Some((10.0, 20.0)),
    size: Some((400.0, 300.0)),
};

fn popup_from(browser: &mut Browser, opener: TabId) -> (TabId, Vec<Effect>) {
    let slot = slot_of(browser, opener).unwrap();
    browser.open_from(
        slot,
        "https://login.test",
        Opening::Popup {
            request: REQUEST,
            placement: POPUP_PLACEMENT,
        },
    )
}

#[test]
fn a_new_window_opens_on_its_one_tab_and_becomes_the_window_in_use() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let first = browser.main_window();

    let (tab, effects) = browser.open_window("https://b.test");

    let window = browser.window_of(tab).unwrap();
    assert_ne!(window, first);
    assert_eq!(browser.main_window(), window);
    assert_eq!(window_tabs(&browser, window), vec![tab]);
    assert_eq!(window_tabs(&browser, first), ids);
    let opened = effects
        .iter()
        .position(|effect| matches!(effect, Effect::OpenWindow { window: w, .. } if *w == window))
        .unwrap();
    let loaded = effects
        .iter()
        .position(|effect| matches!(effect, Effect::EnsureSlot { window: w, .. } if *w == window))
        .unwrap();
    assert!(opened < loaded, "the window exists before a page is put in it");
}

#[test]
fn every_window_shows_its_own_active_tab() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let (tab, _) = browser.open_window("https://b.test");

    assert_eq!(browser.position(ids[0]), Some(Position::Visible));
    assert_eq!(browser.position(tab), Some(Position::Visible));
    assert_ne!(slot_of(&browser, ids[0]), slot_of(&browser, tab));
}

#[test]
fn a_tab_another_window_shows_is_never_evicted() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let (tab, _) = browser.open_window("https://b.test");
    let window = browser.window_of(tab).unwrap();

    browser.open_tab_in(window, "https://c.test", true);

    assert!(slot_of(&browser, ids[0]).is_some(), "still on screen in its window");
}

#[test]
fn a_parked_webview_is_moved_into_the_window_that_reuses_it() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    let first = browser.main_window();
    browser.close_tab(ids[1], HOME).unwrap();
    let parked = browser
        .slots_in(first)
        .into_iter()
        .find(|&slot| Some(slot) != slot_of(&browser, ids[0]))
        .expect("closing a tab parks its webview where it was");

    let (tab, effects) = browser.open_window("https://c.test");

    let window = browser.window_of(tab).unwrap();
    assert_eq!(slot_of(&browser, tab), Some(parked));
    assert!(effects.contains(&Effect::Move { slot: parked, window }));
    assert_eq!(browser.slots_in(window), vec![parked]);
}

#[test]
fn a_popup_opens_in_a_window_of_its_own_connected_to_its_opener() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let first = browser.main_window();

    let (popup, effects) = popup_from(&mut browser, ids[0]);

    let window = browser.window_of(popup).unwrap();
    assert_ne!(window, first);
    assert_eq!(browser.kind(window), Some(WindowKind::Popup));
    assert!(effects.contains(&Effect::OpenWindow {
        window,
        kind: WindowKind::Popup,
        placement: POPUP_PLACEMENT,
    }));
    let slot = slot_of(&browser, popup).unwrap();
    assert!(effects.contains(&Effect::Adopt {
        profile: Profile::Default,
        slot,
        request: REQUEST,
        url: "https://login.test".to_string(),
        window,
    }));
    assert_eq!(
        browser.position(ids[0]),
        Some(Position::Visible),
        "its own window still shows it"
    );
    assert_eq!(browser.main_window(), first, "a popup is not where tabs open");
}

#[test]
fn an_opener_in_the_background_keeps_running_for_its_popup() {
    let (mut browser, ids) = browser_with(3, &["https://a.test"]);
    popup_from(&mut browser, ids[0]);

    browser.open_tab("https://b.test", true);

    assert_eq!(browser.position(ids[0]), Some(Position::MustRun));
}

#[test]
fn a_link_opened_in_a_new_window_is_not_connected() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    let (tab, effects) = browser.open_from(slot, "https://b.test", Opening::Window);

    let window = browser.window_of(tab).unwrap();
    assert_eq!(browser.kind(window), Some(WindowKind::Normal));
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Adopt { .. })));
    browser.open_tab_in(browser.window_of(ids[0]).unwrap(), "https://c.test", true);
    assert_ne!(browser.position(ids[0]), Some(Position::MustRun));
}

#[test]
fn a_link_in_a_popup_opens_in_the_window_used_last_and_brings_it_forward() {
    let (mut browser, ids) = browser_with(3, &["https://a.test"]);
    let first = browser.main_window();
    browser.open_window("https://c.test");
    browser.focus(first);
    let (popup, _) = popup_from(&mut browser, ids[0]);
    let popup_window = browser.window_of(popup).unwrap();
    browser.focus(popup_window);
    let slot = slot_of(&browser, popup).unwrap();

    let (tab, effects) = browser.open_from(slot, "https://b.test", Opening::Foreground);

    assert_eq!(browser.window_of(tab), Some(first));
    assert_eq!(window_tabs(&browser, popup_window), vec![popup]);
    assert!(effects.contains(&Effect::FocusWindow { window: first }));
}

#[test]
fn a_tab_opened_from_a_popup_window_goes_to_the_window_used_last() {
    let (mut browser, ids) = browser_with(3, &["https://a.test"]);
    let first = browser.main_window();
    browser.open_window("https://c.test");
    browser.focus(first);
    let (popup, _) = popup_from(&mut browser, ids[0]);
    let popup_window = browser.window_of(popup).unwrap();
    browser.focus(popup_window);

    let (tab, effects) = browser.open_tab_in(popup_window, HOME, true);

    assert_eq!(browser.window_of(tab), Some(first));
    assert!(effects.contains(&Effect::FocusWindow { window: first }));
}

#[test]
fn closing_a_popup_closes_its_window_and_frees_its_opener() {
    let (mut browser, ids) = browser_with(3, &["https://a.test"]);
    let (popup, _) = popup_from(&mut browser, ids[0]);
    let window = browser.window_of(popup).unwrap();
    let slot = slot_of(&browser, popup).unwrap();
    browser.open_tab("https://b.test", true);

    let effects = browser.close_tab(popup, HOME).unwrap();

    assert!(effects.contains(&Effect::Destroy { slot }));
    assert!(effects.contains(&Effect::CloseWindow { window }));
    assert!(browser.tab(popup).is_err());
    assert!(!browser.window_ids().contains(&window));
    assert_ne!(browser.position(ids[0]), Some(Position::MustRun));
}

#[test]
fn closing_a_window_destroys_its_webviews_and_its_tabs() {
    let (mut browser, ids) = browser_with(3, &["https://a.test"]);
    let (tab, _) = browser.open_window("https://b.test");
    let window = browser.window_of(tab).unwrap();
    let second = browser.open_tab_in(window, "https://c.test", true).0;
    browser.select_tab(tab, 0).unwrap();
    let slots = [slot_of(&browser, tab).unwrap(), slot_of(&browser, second).unwrap()];

    let effects = browser.close_window(window);

    let closed = position(&effects, &Effect::CloseWindow { window });
    for slot in slots {
        let destroyed = position(&effects, &Effect::Destroy { slot });
        assert!(destroyed < closed, "a webview goes before its window does");
    }
    assert!(browser.tab(tab).is_err() && browser.tab(second).is_err());
    assert!(browser.slots_in(window).is_empty());
    assert_eq!(ids_of(&browser), ids);
    assert_eq!(browser.position(ids[0]), Some(Position::Visible));
}

#[test]
fn closing_the_last_browser_window_is_left_to_the_caller() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let window = browser.main_window();

    assert!(browser.is_last_window(window));
    assert!(browser.close_window(window).is_empty());
    assert_eq!(browser.active(), Some(ids[0]));
}

#[test]
fn a_popup_is_never_the_last_window() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let (popup, _) = popup_from(&mut browser, ids[0]);

    assert!(!browser.is_last_window(browser.window_of(popup).unwrap()));
}

#[test]
fn closing_the_last_tab_of_a_window_keeps_the_window() {
    let (mut browser, _) = browser_with(2, &["https://a.test"]);
    let (tab, _) = browser.open_window("https://b.test");
    let window = browser.window_of(tab).unwrap();

    browser.close_tab(tab, HOME).unwrap();

    let state = browser.state_in(window).unwrap();
    assert_eq!(state.tabs.len(), 1);
    assert_eq!(state.tabs[0].url(), HOME);
}

#[test]
fn closing_a_tab_activates_its_neighbour_in_the_same_window() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    let (tab, _) = browser.open_window("https://c.test");
    let window = browser.window_of(tab).unwrap();
    let last = browser.open_tab_in(window, "https://d.test", true).0;

    browser.close_tab(last, HOME).unwrap();

    assert_eq!(browser.active_in(window), Some(tab));
    assert_eq!(browser.active_in(browser.window_of(ids[0]).unwrap()), Some(ids[1]));
}

#[test]
fn picking_by_position_counts_only_the_window_tabs() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    let first = browser.window_of(ids[0]).unwrap();
    let (tab, _) = browser.open_window("https://c.test");
    let window = browser.window_of(tab).unwrap();

    assert_eq!(browser.pick_in(window, Pick::Next), Some(tab));
    assert_eq!(browser.pick_in(window, Pick::Nth(1)), None);
    assert_eq!(browser.pick_in(first, Pick::Last), Some(ids[1]));
}

#[test]
fn reordering_moves_a_tab_among_its_window_tabs() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    let first = browser.window_of(ids[0]).unwrap();
    browser.open_window("https://c.test");

    browser.reorder_tab(ids[1], 0).unwrap();

    assert_eq!(window_tabs(&browser, first), vec![ids[1], ids[0]]);
}

#[test]
fn reopening_a_tab_whose_window_closed_puts_it_in_the_window_used_last() {
    let (mut browser, _) = browser_with(2, &["https://a.test"]);
    let (tab, _) = browser.open_window("https://b.test");
    let window = browser.window_of(tab).unwrap();
    let kept = browser.open_tab_in(window, "https://c.test", true).0;
    browser.close_tab(kept, HOME).unwrap();
    browser.close_window(window);

    browser.reopen_closed_tab();

    let reopened = browser.active().unwrap();
    assert_eq!(browser.tab(reopened).unwrap().url(), "https://c.test");
    assert_eq!(browser.window_of(reopened), Some(browser.main_window()));
}

#[test]
fn tabs_opened_from_nowhere_go_to_the_window_used_last() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let first = browser.window_of(ids[0]).unwrap();
    browser.open_window("https://b.test");

    browser.focus(first);
    let (tab, _) = browser.open_tab("https://c.test", true);

    assert_eq!(browser.window_of(tab), Some(first));
}

#[test]
fn switching_tabs_in_one_window_captures_only_the_page_it_leaves() {
    let (mut browser, ids) = browser_with(3, &["https://a.test"]);
    let (tab, _) = browser.open_window("https://b.test");
    let window = browser.window_of(tab).unwrap();

    let (_, effects) = browser.open_tab_in(window, "https://c.test", true);

    let captured: Vec<TabId> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Leave { tab, capture: true, .. } => Some(*tab),
            _ => None,
        })
        .collect();
    assert_eq!(captured, vec![tab]);
    assert_eq!(browser.position(ids[0]), Some(Position::Visible));
}

#[test]
fn a_tab_whose_window_has_closed_reopens_at_its_place_in_the_window_used_last() {
    let (mut browser, ids) = browser_with(3, &["https://a.test", "https://b.test"]);
    let (tab, _) = browser.open_window("https://c.test");
    let window = browser.window_of(tab).unwrap();
    let closing = browser.open_tab_in(window, "https://d.test", true).0;
    browser.open_tab_in(window, "https://e.test", true);
    browser.close_tab(closing, HOME).unwrap();
    browser.close_window(window);

    browser.reopen_closed_tab();

    let urls: Vec<&str> = browser.tabs().iter().map(Tab::url).collect();
    assert_eq!(urls, ["https://a.test", "https://d.test", "https://b.test"]);
    assert_eq!(window_tabs(&browser, browser.main_window())[0], ids[0]);
}

#[test]
fn a_window_showing_an_internal_page_reserves_no_slot() {
    let mut browser = Browser::new(1);
    browser.open_tab("https://a.test", true);
    browser.open_tab(HOME, true);
    browser.open_window(HOME);
    browser.open_window(HOME);

    assert_eq!(browser.effective_capacity(), 1);
}

#[test]
fn a_tab_requested_from_outside_every_tab_opens_last_in_the_window_used_last() {
    let (mut browser, ids) = browser_with(3, &["https://a.test", "https://b.test"]);
    let first = browser.main_window();
    let (other, _) = browser.open_window("https://c.test");
    browser.focus(first);

    let (tab, effects) = browser.open_requested("https://d.test", Opening::Foreground);

    assert_eq!(browser.window_of(tab), Some(first));
    assert_eq!(window_tabs(&browser, first), vec![ids[0], ids[1], tab]);
    assert_eq!(browser.active_in(first), Some(tab));
    assert!(browser.window_of(other).is_some());
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::FocusWindow { .. })));
}

#[test]
fn a_popup_requested_from_outside_every_tab_keeps_no_tab_running() {
    let (mut browser, ids) = browser_with(3, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();

    let (popup, effects) = browser.open_requested(
        "https://login.test",
        Opening::Popup {
            request: REQUEST,
            placement: POPUP_PLACEMENT,
        },
    );

    let window = browser.window_of(popup).unwrap();
    assert_eq!(browser.kind(window), Some(WindowKind::Popup));
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::Adopt { request, .. } if *request == REQUEST)));
    assert_ne!(browser.position(ids[1]), Some(Position::MustRun));
}

fn crash(browser: &mut Browser, id: TabId) -> Vec<Effect> {
    let slot = slot_of(browser, id).expect("the tab holds a webview");
    browser.page_crashed(slot, id, CrashCause::OutOfMemory)
}

#[test]
fn a_crashed_page_gives_up_its_slot_and_is_not_loaded_again() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = crash(&mut browser, ids[0]);

    assert!(effects.contains(&Effect::Blank { slot }));
    assert_eq!(
        browser.tab(ids[0]).unwrap().presence,
        TabPresence::Crashed {
            cause: CrashCause::OutOfMemory
        }
    );
    browser.select_tab(ids[0], 1).unwrap();
    assert!(
        slot_of(&browser, ids[0]).is_none(),
        "the visible crashed tab stays unloaded"
    );
}

#[test]
fn a_crashed_fixed_tab_is_not_loaded_again() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    browser.set_fixed(ids[0], true).unwrap();

    crash(&mut browser, ids[0]);
    browser.select_tab(ids[1], 1).unwrap();

    assert!(slot_of(&browser, ids[0]).is_none());
}

#[test]
fn reloading_a_crashed_tab_loads_its_page_again() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    crash(&mut browser, ids[0]);

    let effects = browser.reload(ids[0]).unwrap();

    let slot = slot_of(&browser, ids[0]).expect("the tab holds a webview again");
    assert!(effects.contains(&Effect::EnsureSlot {
        profile: Profile::Default,
        slot,
        url: "https://a.test".to_string(),
        window: browser.main_window(),
    }));
}

#[test]
fn navigating_a_crashed_tab_loads_the_new_page() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    crash(&mut browser, ids[0]);

    browser.navigate(ids[0], "https://b.test").unwrap();

    assert!(slot_of(&browser, ids[0]).is_some());
    assert_eq!(browser.tab(ids[0]).unwrap().url(), "https://b.test");
}

#[test]
fn a_crash_releases_the_dialog_the_page_was_waiting_on() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.open_dialog(slot, dialog(1, DialogKind::Alert));

    let effects = crash(&mut browser, ids[0]);

    assert_eq!(answers(&effects).len(), 1);
    assert!(browser.tab(ids[0]).unwrap().dialog.is_none());
}

#[test]
fn a_crash_in_a_slot_no_tab_holds_changes_nothing() {
    let (mut browser, _) = browser_with(2, &["https://a.test"]);

    assert!(browser
        .page_crashed(SlotId(7), TabId(1), CrashCause::Unknown)
        .is_empty());
}

#[test]
fn a_crash_reported_after_the_slot_passed_to_another_tab_is_ignored() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    let slot = slot_of(&browser, ids[1]).unwrap();

    assert!(browser.page_crashed(slot, ids[0], CrashCause::Unknown).is_empty());
    assert_eq!(slot_of(&browser, ids[1]), Some(slot));
}

#[test]
fn a_crashed_tab_reserves_no_slot() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    browser.set_fixed(ids[0], true).unwrap();
    let reserved = browser.reserved();

    crash(&mut browser, ids[0]);

    assert_eq!(browser.reserved(), reserved - 1);
}

// Split policy is exercised through the same window-scoped intents as IPC.
fn split_pair(browser: &mut Browser) -> (WindowId, TabId, TabId) {
    let window = browser.main_window();
    let left = browser.active_in(window).unwrap();
    let (right, _) = browser.open_tab_in(window, "https://right.test", false);
    browser.split_tab(window, right).unwrap();
    (window, left, right)
}

fn pane_tabs(browser: &Browser, window: WindowId) -> Vec<TabId> {
    browser
        .state_in(window)
        .unwrap()
        .viewports
        .iter()
        .map(|pane| pane.tab)
        .collect()
}

#[test]
fn split_and_separate_handoff_native_focus_after_geometry_is_ready() {
    let (mut browser, _) = browser_with(2, &["https://left.test"]);
    let window = browser.main_window();
    let (right, _) = browser.open_tab("https://right.test", false);
    let split = browser.split_tab(window, right).unwrap();
    assert!(split.contains(&Effect::FocusPane {
        window,
        slot: slot_of(&browser, right)
    }));
    assert!(browser
        .resolve_pane_focus(&split, |_| false)
        .iter()
        .all(|effect| !matches!(effect, Effect::FocusPane { .. })));
    assert_eq!(
        browser.resolve_pane_focus(&[], |_| true),
        vec![Effect::FocusPane {
            window,
            slot: slot_of(&browser, right)
        }]
    );
    assert!(browser.resolve_pane_focus(&[], |_| true).is_empty());
    let close = browser.separate_tabs(window, right).unwrap();
    assert!(close.contains(&Effect::FocusPane {
        window,
        slot: slot_of(&browser, right)
    }));
    browser.resolve_pane_focus(&close, |_| false);
    assert_eq!(
        browser.resolve_pane_focus(&[], |_| true),
        vec![Effect::FocusPane {
            window,
            slot: slot_of(&browser, right)
        }]
    );
}

#[test]
fn explicit_pane_focus_waits_for_geometry_and_is_applied_once() {
    use crate::chrome::{Layout, PaneLayout};
    use crate::webview::{Viewport, Viewports};

    for select in [false, true] {
        let (mut browser, _) = browser_with(2, &["https://left.test"]);
        let (window, left, _) = split_pair(&mut browser);
        let effects = if select {
            browser.select_tab(left, 1).unwrap()
        } else {
            browser.focus_pane(window, left).unwrap()
        };
        let viewports = Viewports::default();
        let ready = |target: &ViewportTarget| viewports.can_show(target);
        assert!(browser
            .resolve_pane_focus(&effects, ready)
            .iter()
            .all(|effect| !matches!(effect, Effect::FocusPane { .. })));
        let current = browser.state();
        let mut layout = Layout {
            revision: current.layout_revision - 1,
            viewports: current
                .viewports
                .iter()
                .enumerate()
                .map(|(index, pane)| PaneLayout {
                    id: pane.id.clone(),
                    rect: Viewport {
                        x: index as f64 * 404.0,
                        y: 80.0,
                        width: 400.0,
                        height: 600.0,
                    },
                    covered: false,
                    radius: 0.0,
                })
                .collect(),
            overlays: Vec::new(),
        };
        assert!(!viewports
            .record(window, &layout, &current.viewports, current.layout_revision)
            .unwrap());
        assert!(browser.resolve_pane_focus(&[], ready).is_empty());
        layout.revision = current.layout_revision;
        layout.viewports[0].rect.width = 0.0;
        viewports
            .record(window, &layout, &current.viewports, current.layout_revision)
            .unwrap();
        assert!(browser.resolve_pane_focus(&[], ready).is_empty());
        layout.viewports[0].rect.width = 400.0;
        viewports
            .record(window, &layout, &current.viewports, current.layout_revision)
            .unwrap();
        let show = browser.reconcile();
        let applied = browser.resolve_pane_focus(&show, ready);
        let slot = slot_of(&browser, left);
        assert_eq!(applied.last(), Some(&Effect::FocusPane { window, slot }));
        assert!(applied[..applied.len() - 1].contains(&Effect::Show { slot: slot.unwrap() }));
        assert!(browser.resolve_pane_focus(&[], ready).is_empty());
    }
}

#[test]
fn deferred_pane_focus_is_dropped_when_assignment_or_focus_changes() {
    for replace in [false, true] {
        let (mut browser, _) = browser_with(3, &["https://left.test"]);
        let (window, left, right) = split_pair(&mut browser);
        let effects = browser.focus_pane(window, left).unwrap();
        browser.resolve_pane_focus(&effects, |_| false);
        if replace {
            browser.open_tab("https://replacement.test", true);
        } else {
            browser.focus_slot(slot_of(&browser, right).unwrap());
        }
        assert!(browser.resolve_pane_focus(&[], |_| true).is_empty());
    }
}

#[test]
fn split_intents_export_ratio_focus_and_stable_surviving_pane_ids() {
    let (mut browser, _) = browser_with(1, &[HOME]);
    let original = browser.state();
    let (window, left, right) = split_pair(&mut browser);
    let split = browser.state();
    assert_eq!(pane_tabs(&browser, window), [left, right]);
    assert_eq!(split.active, Some(right));
    assert_eq!(split.splits[0].ratio, 0.5);
    assert_eq!(split.layout_revision, original.layout_revision + 1);
    for ratio in [0.2, 0.8, 0.5] {
        let before = browser.state().layout_revision;
        browser.set_split_ratio(window, left, ratio).unwrap();
        assert_eq!(browser.state().splits[0].ratio, ratio);
        assert_eq!(browser.state().layout_revision, before + 1);
        browser.set_split_ratio(window, right, ratio).unwrap();
        assert_eq!(browser.state().layout_revision, before + 1);
    }
    browser.focus_pane(window, left).unwrap();
    assert_eq!(browser.active(), Some(left));
    assert_eq!(browser.state().layout_revision, split.layout_revision + 3);
    assert!(browser.focus_pane(window, left).unwrap().is_empty());
    browser.separate_tabs(window, right).unwrap();
    let survivor = browser.state();
    assert_eq!(survivor.viewports[0].id, split.viewports[0].id);
    assert_eq!(survivor.active, Some(left));
    assert_eq!(survivor.splits.len(), 0);
    assert!(browser.tab(left).is_ok());
    browser.split_tab(window, right).unwrap();
    assert_eq!(pane_tabs(&browser, window), [left, right]);
    assert_ne!(browser.state().viewports[1].id, original.viewports[0].id);
    assert_eq!(browser.state().splits[0].ratio, 0.5);
}

#[test]
fn split_intents_reject_invalid_targets_without_mutation() {
    let (mut browser, ids) = browser_with(1, &[HOME]);
    let window = browser.main_window();
    let (hidden, _) = browser.open_tab("https://hidden.test", false);
    let (foreign, _) = browser.open_window("https://foreign.test");
    let foreign_window = browser.window_of(foreign).unwrap();
    let (popup, _) = browser.open_from(
        slot_of(&browser, foreign).unwrap(),
        "https://popup.test",
        Opening::Popup {
            request: WindowRequestId(30),
            placement: Placement::default(),
        },
    );
    let popup_window = browser.window_of(popup).unwrap();
    for split in [false, true] {
        if split {
            browser.split_tab(window, hidden).unwrap();
        }
        let (other_hidden, _) = browser.open_tab_in(window, "https://other.test", false);
        for command in ["split", "close", "ratio", "focus"] {
            let mut cases = vec![
                (window, foreign),
                (window, popup),
                (window, TabId(u64::MAX)),
                (popup_window, popup),
                (WindowId(999), ids[0]),
            ];
            if command == "split" {
                cases.push((window, ids[0])); // duplicate assignment
                if split {
                    cases.push((window, other_hidden));
                } // third pane
            } else {
                cases.push((window, other_hidden));
                if !split && command != "focus" {
                    cases.push((window, ids[0]));
                }
            }
            for (caller, tab) in cases {
                let before = serde_json::to_value([
                    browser.state_in(window),
                    browser.state_in(foreign_window),
                    browser.state_in(popup_window),
                ])
                .unwrap();
                let result = match command {
                    "split" => browser.split_tab(caller, tab),
                    "close" => browser.separate_tabs(caller, tab),
                    "ratio" => browser.set_split_ratio(caller, tab, 0.5),
                    _ => browser.focus_pane(caller, tab),
                };
                assert!(result.is_err(), "accepted {command}({caller:?}, {tab:?})");
                assert_eq!(
                    serde_json::to_value([
                        browser.state_in(window),
                        browser.state_in(foreign_window),
                        browser.state_in(popup_window)
                    ])
                    .unwrap(),
                    before
                );
            }
        }
    }
    for ratio in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.199, 0.801] {
        let before = serde_json::to_value(browser.state_in(window)).unwrap();
        assert!(browser.set_split_ratio(window, ids[0], ratio).is_err());
        assert_eq!(serde_json::to_value(browser.state_in(window)).unwrap(), before);
    }
}

#[test]
fn split_selection_leaves_pairs_intact_and_only_focuses_visible_members() {
    let (mut browser, _) = browser_with(3, &["https://left.test"]);
    let (window, left, right) = split_pair(&mut browser);
    let revision = browser.state().layout_revision;
    let slots = [slot_of(&browser, left), slot_of(&browser, right)];
    let effects = browser.select_tab(left, 10).unwrap();
    assert_eq!(pane_tabs(&browser, window), [left, right]);
    assert_eq!(browser.active(), Some(left));
    assert!(effects.iter().all(|effect| !matches!(
        effect,
        Effect::Leave { .. } | Effect::Blank { .. } | Effect::Freeze { .. } | Effect::EnsureSlot { .. }
    )));
    assert_eq!([slot_of(&browser, left), slot_of(&browser, right)], slots);
    let (hidden, _) = browser.open_tab("https://hidden.test", false);
    browser.select_tab(hidden, 11).unwrap();
    assert_eq!(pane_tabs(&browser, window), [hidden]);
    assert_eq!(browser.state().layout_revision, revision + 1);
    browser.select_tab(right, 12).unwrap();
    assert_eq!(pane_tabs(&browser, window), [left, right]);
    browser.select_tab(hidden, 13).unwrap();
    assert!(!browser.is_visible(left));
    assert_eq!(browser.pick_in(window, Pick::Previous), Some(right));
}

#[test]
fn split_foreground_and_reopened_tabs_show_alone_preserving_pairs() {
    let (mut browser, _) = browser_with(3, &["https://left.test"]);
    let (window, left, right) = split_pair(&mut browser);
    let (opened, _) = browser.open_tab("https://opened.test", true);
    assert_eq!(pane_tabs(&browser, window), [opened]);
    browser.close_tab(opened, HOME).unwrap();
    assert_eq!(pane_tabs(&browser, window), [left, right]);
    browser.reopen_closed_tab();
    let reopened = browser.active().unwrap();
    assert_eq!(browser.tab(reopened).unwrap().url(), "https://opened.test");
    assert_eq!(pane_tabs(&browser, window), [reopened]);
}

#[test]
fn split_page_opens_show_alone_and_background_opens_preserve_focus() {
    let (mut browser, _) = browser_with(4, &["https://left.test"]);
    let (window, left, right) = split_pair(&mut browser);
    let source = slot_of(&browser, left).unwrap();
    let (background, _) = browser.open_from(source, "https://background.test", Opening::Background);
    assert_eq!(pane_tabs(&browser, window), [left, right]);
    assert_eq!(browser.active(), Some(right));
    let (opened, _) = browser.open_from(source, "https://opened.test", Opening::Connected(WindowRequestId(5)));
    assert_eq!(pane_tabs(&browser, window), [opened]);
    assert_eq!(browser.active(), Some(opened));
    assert!(browser.connected.contains(&opened));
    assert_eq!(browser.openers[&opened], left);
    let ids: Vec<_> = browser.tabs_in(window).map(|tab| tab.id).collect();
    assert_eq!(ids, [left, right, background, opened]);
}

#[test]
fn repeated_background_opens_from_either_pair_member_keep_click_order() {
    for half in [0, 1] {
        let (mut browser, _) = browser_with(4, &["https://left.test"]);
        let (window, left, right) = split_pair(&mut browser);
        let source = slot_of(&browser, [left, right][half]).unwrap();
        let mut expected = vec![left, right];
        for url in ["https://c.test", "https://d.test", "https://e.test"] {
            let (child, _) = browser.open_from(source, url, Opening::Background);
            expected.push(child);
            assert_eq!(browser.tabs_in(window).map(|tab| tab.id).collect::<Vec<_>>(), expected);
            assert_eq!(pane_tabs(&browser, window), [left, right]);
            assert_eq!(browser.active(), Some(right));
        }
    }
}

#[test]
fn split_closing_either_member_focuses_the_survivor_even_with_other_candidates() {
    for close_left in [false, true] {
        let (mut browser, _) = browser_with(4, &["https://other.test", "https://left.test"]);
        let (window, left, right) = split_pair(&mut browser);
        let revision = browser.state().layout_revision;
        let (closed, survivor) = if close_left { (left, right) } else { (right, left) };
        browser.close_tab(closed, HOME).unwrap();
        assert_eq!(pane_tabs(&browser, window), [survivor]);
        assert_eq!(browser.active(), Some(survivor));
        assert!(browser.state().splits.is_empty());
        assert_eq!(browser.state().layout_revision, revision + 1);
    }
}

#[test]
fn split_closing_a_tab_with_a_visible_opener_collapses_to_that_opener() {
    let (mut browser, _) = browser_with(3, &["https://left.test"]);
    let (window, left, right) = split_pair(&mut browser);
    browser.open_tab("https://hidden.test", false);
    browser.openers.insert(left, right);
    let survivor = browser.state().viewports[1].id.clone();
    browser.close_tab(left, HOME).unwrap();
    assert_eq!(pane_tabs(&browser, window), [right]);
    assert_eq!(browser.state().focused_viewport, survivor);
}

#[test]
fn split_closing_hidden_tabs_preserves_assignments_and_focus_and_last_visible_collapses() {
    let (mut browser, _) = browser_with(3, &["https://left.test"]);
    let (window, left, right) = split_pair(&mut browser);
    let (hidden, _) = browser.open_tab("https://hidden.test", false);
    let revision = browser.state().layout_revision;
    browser.close_tab(hidden, HOME).unwrap();
    assert_eq!(pane_tabs(&browser, window), [left, right]);
    assert_eq!(browser.active(), Some(right));
    assert_eq!(browser.state().layout_revision, revision);
    browser.close_tab(right, HOME).unwrap();
    assert_eq!(pane_tabs(&browser, window), [left]);
    assert_eq!(browser.active(), Some(left));
    assert_eq!(browser.state().layout_revision, revision + 1);
}

#[test]
fn split_native_focus_switches_between_panes_without_leaving_or_freezing_either() {
    let (mut browser, _) = browser_with(1, &["https://left.test"]);
    let (window, left, right) = split_pair(&mut browser);
    browser.set_optimization(1, Policy::Always, Policy::Always);
    for tab in [left, right, left] {
        let before = browser.state().layout_revision;
        let slot = slot_of(&browser, tab).unwrap();
        let target = browser.focus_slot(slot).unwrap();
        assert_eq!(browser.active(), Some(tab));
        assert_eq!(target.revision, before);
        assert!(browser.focus_pane(window, tab).unwrap().is_empty());
        let effects = browser.reconcile();
        assert!(effects.iter().all(|effect| !matches!(
            effect,
            Effect::Leave { .. } | Effect::Blank { .. } | Effect::Freeze { .. }
        )));
    }
}

#[test]
fn split_reservations_cover_two_windows_and_a_popup_at_capacity_one() {
    let (mut browser, _) = browser_with(1, &["https://first.test"]);
    let (first_window, left, right) = split_pair(&mut browser);
    let (other, _) = browser.open_window("https://other.test");
    let (popup, _) = browser.open_from(
        slot_of(&browser, other).unwrap(),
        "https://popup.test",
        Opening::Popup {
            request: WindowRequestId(9),
            placement: Placement::default(),
        },
    );
    assert_eq!(browser.reserved(), 4);
    browser.set_capacity(1);
    for tab in [left, right, other, popup] {
        assert!(slot_of(&browser, tab).is_some());
        assert_eq!(browser.position(tab), Some(Position::Visible));
    }
    assert_eq!(browser.state_in(first_window).unwrap().viewports.len(), 2);
    assert_eq!(
        browser
            .state_in(browser.window_of(popup).unwrap())
            .unwrap()
            .viewports
            .len(),
        1
    );
}

#[test]
fn split_reservations_preserve_fixed_overlap_and_other_must_run_counts() {
    let (mut browser, ids) = browser_with(
        8,
        &[
            "https://audio.test",
            "https://capture.test",
            "https://fixed.test",
            "https://left.test",
        ],
    );
    browser.report_audio(slot_of(&browser, ids[0]).unwrap(), true, 1);
    browser.report_state(
        ids[1],
        Some(PageState {
            url: "https://capture.test".into(),
            capturing: true,
            ..PageState::default()
        }),
    );
    browser.set_fixed(ids[2], true).unwrap();
    let (window, left, right) = split_pair(&mut browser);
    browser.set_fixed(left, true).unwrap();
    assert_eq!(browser.reserved(), 2 + 2 + 2);
    browser.set_capacity(1);
    assert_eq!(browser.effective_capacity(), 6);
    for tab in [ids[0], ids[1], ids[2], left, right] {
        assert!(slot_of(&browser, tab).is_some());
    }
    browser.separate_tabs(window, right).unwrap();
    assert_eq!(browser.reserved(), 1 + 2 + 2);
}

#[test]
fn split_always_policies_keep_both_visible_pages_and_connected_openers_running() {
    for (freeze, discard) in [(Policy::Always, Policy::Never), (Policy::Never, Policy::Always)] {
        let (mut browser, ids) = browser_with(
            8,
            &[
                "https://audio.test",
                "https://capture.test",
                "https://opener.test",
                "https://left.test",
            ],
        );
        browser.report_audio(slot_of(&browser, ids[0]).unwrap(), true, 1);
        browser.report_state(
            ids[1],
            Some(PageState {
                url: "https://capture.test".into(),
                capturing: true,
                ..PageState::default()
            }),
        );
        browser.open_from(
            slot_of(&browser, ids[2]).unwrap(),
            "https://connected.test",
            Opening::Background,
        );
        let child = *browser.openers.keys().next().unwrap();
        browser.connected.insert(child);
        let (_, left, right) = split_pair(&mut browser);
        browser.set_optimization(1, freeze, discard);
        assert_eq!(browser.reserved(), 3);
        for tab in [left, right, ids[2]] {
            assert!(matches!(browser.tab(tab).unwrap().presence, TabPresence::Live { .. }));
        }
        for tab in [ids[0], ids[1]] {
            assert!(!matches!(browser.tab(tab).unwrap().presence, TabPresence::Live { .. }));
        }
    }
}

#[test]
fn split_pressure_discards_background_work_but_preserves_both_panes() {
    let (mut browser, ids) = browser_with(5, &["https://work.test", "https://left.test"]);
    browser.report_state(
        ids[0],
        Some(PageState {
            url: "https://work.test".into(),
            unsaved: true,
            ..PageState::default()
        }),
    );
    let (_, left, right) = split_pair(&mut browser);
    browser.set_optimization(5, Policy::Smart, Policy::Smart);
    browser.tick(100_000, Pressure::Critical, HashMap::new());
    assert_eq!(browser.tab(ids[0]).unwrap().presence, TabPresence::Discarded);
    assert!(browser.tab(ids[0]).unwrap().relieved);
    for tab in [left, right] {
        assert!(matches!(browser.tab(tab).unwrap().presence, TabPresence::Live { .. }));
    }
}

#[test]
fn split_crash_releases_only_the_failed_panes_reservation_until_reload() {
    let (mut browser, _) = browser_with(1, &["https://left.test"]);
    let (_, left, right) = split_pair(&mut browser);
    let healthy = slot_of(&browser, left).unwrap();
    let failed = slot_of(&browser, right).unwrap();
    let effects = browser.page_crashed(failed, right, CrashCause::Ended);
    assert!(browser.tab(right).unwrap().is_crashed());
    assert_eq!(browser.reserved(), 1);
    assert_eq!(slot_of(&browser, left), Some(healthy));
    assert!(effects
        .iter()
        .all(|effect| !matches!(effect, Effect::Blank {slot} | Effect::Freeze {slot} if *slot == healthy)));
    browser.reload(right).unwrap();
    assert_eq!(browser.reserved(), 2);
    assert!(slot_of(&browser, right).is_some());
}

#[test]
fn split_failed_acquisition_keeps_the_assignment_and_healthy_sibling() {
    let (mut browser, _) = browser_with(1, &["https://left.test"]);
    let (window, left, right) = split_pair(&mut browser);
    let healthy = slot_of(&browser, left).unwrap();
    browser.pool.release(right);
    browser.tab_mut(right).unwrap().lose_page();
    // Force the pool's defensive refusal: all resident slots are protected.
    // Ordinary reservation arithmetic prevents this saturation.
    let parked = browser
        .pool
        .slots()
        .iter()
        .find(|slot| slot.occupant.is_none())
        .unwrap()
        .id;
    browser.pool.remove(parked);
    browser.pool.acquire_new(left, Profile::Default);
    assert_eq!(
        browser.pool.acquire(right, 2, None, Profile::Default),
        Err(HakuError::NoSlotAvailable)
    );
    assert!(browser.bind(right).is_empty());
    assert_eq!(browser.tab(right).unwrap().presence, TabPresence::Discarded);
    assert_eq!(pane_tabs(&browser, window), [left, right]);
    assert_eq!(slot_of(&browser, left), Some(healthy));
}

#[test]
fn native_creation_failure_keeps_the_split_assigned_and_reload_recreates_only_its_slot() {
    let (mut browser, ids) = browser_with(1, &["https://left.test"]);
    let window = browser.main_window();
    let left = ids[0];
    let healthy = slot_of(&browser, left).unwrap();
    let (right, _) = browser.open_tab("https://right.test", false);
    let effects = browser.split_tab(window, right).unwrap();
    let failed = slot_of(&browser, right).unwrap();
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::EnsureSlot { slot, .. } if *slot == failed)));
    // Simulate the native executor reporting that EnsureSlot failed.
    assert_eq!(browser.slots_failed([failed]), vec![Effect::Destroy { slot: failed }]);
    assert_eq!(browser.tab(right).unwrap().presence, TabPresence::Discarded);
    assert!(!browser.tab(right).unwrap().restoring);
    assert_eq!(pane_tabs(&browser, window), [left, right]);
    assert_eq!(browser.active(), Some(right));
    assert_eq!(slot_of(&browser, left), Some(healthy));
    assert!(!browser.slot_urls.contains_key(&failed));
    assert!(!browser.slot_windows.contains_key(&failed));
    assert!(!browser.awaiting.contains(&failed));
    assert!(!browser.slots().iter().any(|slot| slot.id == failed));
    let retry = browser.reload(right).unwrap();
    assert!(retry
        .iter()
        .any(|effect| matches!(effect, Effect::EnsureSlot { url, .. } if url == "https://right.test")));
    assert!(retry.iter().all(|effect| !matches!(effect,
        Effect::EnsureSlot { slot, .. } | Effect::Blank { slot } | Effect::Destroy { slot } | Effect::Freeze { slot }
        if *slot == healthy
    )));
}

#[test]
fn native_creation_and_adoption_failures_are_retryable_in_a_single_pane() {
    for adopt in [false, true] {
        let (mut browser, ids) = browser_with(2, &["https://opener.test"]);
        let opener = slot_of(&browser, ids[0]).unwrap();
        let (tab, effects) = if adopt {
            browser.open_from(opener, "https://retry.test", Opening::Connected(WindowRequestId(1)))
        } else {
            browser.open_tab("https://retry.test", true)
        };
        assert!(effects.iter().any(|effect| if adopt {
            matches!(effect, Effect::Adopt { .. })
        } else {
            matches!(effect, Effect::EnsureSlot { .. })
        }));
        let failed = slot_of(&browser, tab).unwrap();
        browser.slots_failed([failed]);
        assert_eq!(browser.tab(tab).unwrap().presence, TabPresence::Discarded);
        assert!(!browser.connected.contains(&tab));
        assert_eq!(browser.active(), Some(tab));
        let retry = browser.reload(tab).unwrap();
        assert!(retry
            .iter()
            .any(|effect| matches!(effect, Effect::EnsureSlot { url, .. } if url == "https://retry.test")));
    }
}

#[test]
fn split_explicit_focus_emits_only_the_chosen_page_effect_and_repeating_is_a_noop() {
    let (mut browser, _) = browser_with(3, &["https://left.test"]);
    let (window, left, _) = split_pair(&mut browser);
    let effects = browser.focus_pane(window, left).unwrap();
    assert_eq!(
        effects,
        vec![Effect::FocusPane {
            window,
            slot: slot_of(&browser, left)
        }]
    );
    assert!(browser.focus_pane(window, left).unwrap().is_empty());
}

#[test]
fn split_closing_a_visible_tab_trims_the_removed_panes_surplus_slot() {
    let (mut browser, _) = browser_with(1, &["https://left.test"]);
    let (_, left, right) = split_pair(&mut browser);
    let healthy = slot_of(&browser, left).unwrap();
    browser.close_tab(right, HOME).unwrap();
    assert_eq!(browser.slots().len(), 1);
    assert_eq!(slot_of(&browser, left), Some(healthy));
}

#[test]
fn background_pairs_keep_independent_ratios_without_geometry_or_focus_changes() {
    let (mut browser, _) = browser_with(1, &["https://a.test"]);
    let (window, left, right) = split_pair(&mut browser);
    let (single, _) = browser.open_tab("https://single.test", true);
    let (_, second_left, second_right) = split_pair(&mut browser);
    let shown = browser.state();
    browser.set_split_ratio(window, left, 0.7).unwrap();
    assert_eq!(browser.state().layout_revision, shown.layout_revision);
    assert_eq!(pane_tabs(&browser, window), [second_left, second_right]);
    assert_eq!(browser.state().splits[0].ratio, 0.7);
    assert_eq!(browser.state().splits[1].ratio, 0.5);
    browser.select_tab(right, 1).unwrap();
    assert_eq!(pane_tabs(&browser, window), [left, right]);
    assert_eq!(browser.state().layout_revision, shown.layout_revision + 1);
    let ids: Vec<_> = browser.state().viewports.iter().map(|pane| pane.id.clone()).collect();
    assert!(ids.iter().all(|id| shown.viewports.iter().all(|pane| pane.id != *id)));
    browser.separate_tabs(window, second_left).unwrap();
    assert_eq!(browser.state().layout_revision, shown.layout_revision + 1);
    assert_eq!(browser.active(), Some(right));
    browser.select_tab(single, 2).unwrap();
    assert_eq!(browser.state().splits.len(), 1);
    assert_eq!(browser.reserved(), 1);
}

#[test]
fn reordering_either_half_moves_the_pair_and_normalizes_illegal_gaps() {
    for half in [0, 1] {
        let (mut browser, ids) = browser_with(
            5,
            &[
                "https://a.test",
                "https://b.test",
                "https://c.test",
                "https://d.test",
                "https://e.test",
            ],
        );
        let window = browser.main_window();
        browser.select_tab(ids[0], 0).unwrap();
        browser.split_tab(window, ids[1]).unwrap();
        browser.select_tab(ids[2], 0).unwrap();
        browser.split_tab(window, ids[3]).unwrap();
        browser.reorder_tab(ids[half], 1).unwrap();
        assert_eq!(
            browser.tabs_in(window).map(|tab| tab.id).collect::<Vec<_>>(),
            [ids[2], ids[3], ids[0], ids[1], ids[4]]
        );
        browser.reorder_tab(ids[half], 0).unwrap();
        assert_eq!(browser.tabs_in(window).map(|tab| tab.id).collect::<Vec<_>>(), ids);
    }
}

#[test]
fn selecting_a_background_pair_requests_native_focus_after_its_new_geometry() {
    let (mut browser, _) = browser_with(2, &["https://left.test"]);
    let (window, left, _) = split_pair(&mut browser);
    browser.open_tab("haku://settings", true);
    let effects = browser.select_tab(left, 1).unwrap();
    assert!(effects.contains(&Effect::FocusPane {
        window,
        slot: slot_of(&browser, left)
    }));
    assert!(!browser
        .resolve_pane_focus(&effects, |_| false)
        .iter()
        .any(|effect| matches!(effect, Effect::FocusPane { .. })));
    assert!(browser.resolve_pane_focus(&[], |_| true).contains(&Effect::FocusPane {
        window,
        slot: slot_of(&browser, left)
    }));
}

#[test]
fn leaving_a_pair_starts_grace_for_both_members() {
    let (mut browser, _) = browser_with(3, &["https://left.test"]);
    let (_, left, right) = split_pair(&mut browser);
    let (single, _) = browser.open_tab("haku://settings", false);
    browser.set_optimization(3, Policy::Smart, Policy::Smart);
    browser.tick(120_000, Pressure::Normal, HashMap::new());
    browser.select_tab(single, 124_000).unwrap();
    browser.tick(180_000, Pressure::Normal, HashMap::new());
    for member in [left, right] {
        assert_eq!(browser.tab(member).unwrap().active_at, 124_000);
        assert!(
            slot_of(&browser, member).is_some(),
            "member should get grace when leaving visibility"
        );
    }
    browser.tick(124_000 + GRACE_MS, Pressure::Normal, HashMap::new());
    for member in [left, right] {
        assert!(
            slot_of(&browser, member).is_none(),
            "grace should expire from departure"
        );
    }
}

#[test]
fn leaving_a_pair_trims_surplus_slots_when_discard_is_disabled() {
    for transition in ["select", "open", "page open", "separate", "close"] {
        let (mut browser, _) = browser_with(1, &["https://left.test"]);
        browser.set_optimization(1, Policy::Never, Policy::Never);
        let (window, left, right) = split_pair(&mut browser);
        assert_eq!(browser.slots().len(), 2);
        let effects = match transition {
            "select" => {
                let (single, _) = browser.open_tab("https://single.test", false);
                browser.select_tab(single, 1).unwrap()
            }
            "open" => browser.open_tab("https://single.test", true).1,
            "page open" => {
                browser
                    .open_from(
                        slot_of(&browser, left).unwrap(),
                        "https://single.test",
                        Opening::Foreground,
                    )
                    .1
            }
            "separate" => browser.separate_tabs(window, right).unwrap(),
            "close" => browser.close_tab(left, HOME).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(browser.reserved(), 1);
        assert_eq!(browser.slots().len(), 1, "{transition} should trim the surplus slot");
        assert_eq!(browser.state().live_count, 1);
        assert!(effects.iter().any(|effect| matches!(effect, Effect::Destroy { .. })));
        assert!(slot_of(&browser, browser.active().unwrap()).is_some());
    }
}

#[test]
fn background_pair_close_and_separation_keep_the_shown_layout_and_selection() {
    for close in [false, true] {
        for half in [0, 1] {
            let (mut browser, _) = browser_with(4, &["https://left.test"]);
            let (window, left, right) = split_pair(&mut browser);
            let (single, _) = browser.open_tab("haku://settings", true);
            let before = browser.state();
            let member = [left, right][half];
            if close {
                browser.close_tab(member, HOME).unwrap();
            } else {
                browser.separate_tabs(window, member).unwrap();
            }
            assert_eq!(browser.active(), Some(single));
            assert_eq!(pane_tabs(&browser, window), [single]);
            assert_eq!(browser.state().layout_revision, before.layout_revision);
            assert!(browser.state().splits.is_empty());
            assert!(browser.tab([left, right][1 - half]).is_ok());
            assert_eq!(browser.tab(member).is_ok(), !close);
        }
    }
}

#[test]
fn creation_revalidates_a_background_paired_target_without_mutation() {
    let (mut browser, _) = browser_with(1, &[HOME]);
    let (window, left, _) = split_pair(&mut browser);
    browser.open_tab(HOME, true);
    let before = serde_json::to_value(browser.state()).unwrap();
    assert!(browser.split_tab(window, left).is_err());
    assert_eq!(serde_json::to_value(browser.state()).unwrap(), before);
}

#[test]
fn page_insertion_from_either_half_and_reopen_never_break_adjacency() {
    for half in [0, 1] {
        let (mut browser, ids) = browser_with(5, &["https://left.test", "https://closed.test", "https://right.test"]);
        let window = browser.main_window();
        browser.close_tab(ids[1], HOME).unwrap();
        browser.select_tab(ids[0], 0).unwrap();
        browser.split_tab(window, ids[2]).unwrap();
        let member = [ids[0], ids[2]][half];
        browser.open_from(
            slot_of(&browser, member).unwrap(),
            "https://child.test",
            Opening::Background,
        );
        assert_eq!(
            browser.tabs_in(window).take(2).map(|tab| tab.id).collect::<Vec<_>>(),
            [ids[0], ids[2]]
        );
        browser.reopen_closed_tab();
        assert_eq!(
            browser.tabs_in(window).take(2).map(|tab| tab.id).collect::<Vec<_>>(),
            [ids[0], ids[2]]
        );
        assert_eq!(pane_tabs(&browser, window), [browser.active().unwrap()]);
        assert_eq!(browser.state().splits.len(), 1);
    }
}

#[test]
fn background_pair_members_have_no_visible_reservation_across_windows_and_popup() {
    let (mut browser, _) = browser_with(1, &["https://left.test"]);
    let (window, left, right) = split_pair(&mut browser);
    let (single, _) = browser.open_tab("https://single.test", true);
    let (other, _) = browser.open_window("https://other.test");
    let (popup, _) = browser.open_from(
        slot_of(&browser, other).unwrap(),
        "https://popup.test",
        Opening::Popup {
            request: WindowRequestId(82),
            placement: Placement::default(),
        },
    );
    browser.set_capacity(1);
    assert_eq!(browser.reserved(), 3);
    for member in [left, right] {
        assert!(!browser.is_visible(member));
        assert_eq!(slot_of(&browser, member), None);
    }
    for member in [single, other, popup] {
        assert!(slot_of(&browser, member).is_some());
    }
    assert_eq!(browser.state_in(window).unwrap().splits.len(), 1);
}

#[test]
fn foreground_page_opens_from_either_member_leave_the_pair_and_close_back_to_its_opener() {
    for half in [0, 1] {
        let (mut browser, _) = browser_with(4, &["https://left.test"]);
        let (window, left, right) = split_pair(&mut browser);
        let parent = [left, right][half];
        let (opened, _) = browser.open_from(
            slot_of(&browser, parent).unwrap(),
            "https://child.test",
            Opening::Foreground,
        );
        assert_eq!(pane_tabs(&browser, window), [opened]);
        assert_eq!(browser.openers[&opened], parent);
        assert_eq!(browser.state().splits.len(), 1);
        browser.close_tab(opened, HOME).unwrap();
        assert_eq!(pane_tabs(&browser, window), [left, right]);
        assert_eq!(browser.active(), Some(parent));
    }
}

#[test]
fn reopening_a_closed_member_restores_only_an_unpaired_tab() {
    for half in [0, 1] {
        let (mut browser, _) = browser_with(2, &["https://left.test"]);
        let (window, left, right) = split_pair(&mut browser);
        let closed = [left, right][half];
        let url = browser.tab(closed).unwrap().url().to_owned();
        browser.close_tab(closed, HOME).unwrap();
        browser.reopen_closed_tab();
        assert!(browser.state().splits.is_empty());
        let reopened = browser.active().unwrap();
        assert_eq!(browser.tab(reopened).unwrap().url(), url);
        assert_eq!(pane_tabs(&browser, window), [reopened]);
    }
}

#[test]
fn creating_a_pair_moves_a_left_partner_to_the_selected_tabs_right() {
    let (mut browser, ids) = browser_with(
        3,
        &["https://partner.test", "https://middle.test", "https://selected.test"],
    );
    let window = browser.main_window();
    browser.split_tab(window, ids[0]).unwrap();
    assert_eq!(
        browser.tabs_in(window).map(|tab| tab.id).collect::<Vec<_>>(),
        [ids[1], ids[2], ids[0]]
    );
    assert_eq!(pane_tabs(&browser, window), [ids[2], ids[0]]);
    browser.reorder_tab(ids[1], 1).unwrap();
    assert_eq!(
        browser.tabs_in(window).map(|tab| tab.id).collect::<Vec<_>>(),
        [ids[2], ids[0], ids[1]]
    );
}

/// What a download in `slot` holds back, of the given profile.
fn downloading(slot: SlotId, private: bool) -> Downloading {
    Downloading {
        slots: HashSet::from([slot]),
        ordinary: usize::from(!private),
        private: usize::from(private),
    }
}

fn destroys(effects: &[Effect], slot: SlotId) -> bool {
    effects.contains(&Effect::Destroy { slot })
}

#[test]
fn a_parked_webview_running_a_download_is_kept_under_pressure() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.set_downloading(downloading(slot, false));
    browser.set_pressure(Pressure::Tight);

    let effects = browser.set_optimization(1, Policy::Always, Policy::Always);

    assert!(slot_of(&browser, ids[0]).is_none(), "the tab is still discarded");
    assert!(!destroys(&effects, slot));
    assert!(browser.slot_ids().contains(&slot));
}

#[test]
fn a_download_ending_lets_its_webview_go() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.set_downloading(downloading(slot, false));
    browser.set_pressure(Pressure::Tight);
    browser.set_optimization(1, Policy::Always, Policy::Always);

    let effects = browser.set_downloading(Downloading::default());

    assert!(destroys(&effects, slot));
}

#[test]
fn a_tab_of_the_same_profile_loads_into_a_webview_running_a_download() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.set_downloading(downloading(slot, false));

    let (next, effects) = browser.open_tab("https://b.test", true);

    assert!(!destroys(&effects, slot));
    assert_eq!(slot_of(&browser, next), Some(slot));
}

#[test]
fn a_private_tab_never_replaces_an_ordinary_webview_running_a_download() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.set_downloading(downloading(slot, false));
    browser.open_tab("https://b.test", true);

    let (private, effects) = browser.open_private_window("https://secret.test");

    assert!(!destroys(&effects, slot));
    assert!(browser.slot_ids().contains(&slot));
    assert!(slot_of(&browser, private).is_some_and(|private| private != slot));
}

#[test]
fn lowering_capacity_keeps_a_webview_running_a_download() {
    let (mut browser, ids) = browser_with(3, &["https://a.test", "https://b.test", "https://c.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.set_downloading(downloading(slot, false));

    let effects = browser.set_capacity(1);

    assert!(!destroys(&effects, slot));
    assert!(browser.slot_ids().contains(&slot));
}

#[test]
fn closing_a_window_moves_its_downloading_webview_to_another() {
    let mut browser = Browser::new(2);
    let (stays, _) = browser.open_tab("https://a.test", true);
    let (closing, _) = browser.open_window("https://b.test");
    let window = browser.window_of(closing).unwrap();
    let slot = slot_of(&browser, closing).unwrap();
    browser.set_downloading(downloading(slot, false));

    let effects = browser.close_window(window);

    let home = browser.window_of(stays).unwrap();
    assert!(effects.contains(&Effect::Move { slot, window: home }));
    assert!(!destroys(&effects, slot));
    assert!(browser.slot_ids().contains(&slot));
    assert!(effects.contains(&Effect::CloseWindow { window }));
}

#[test]
fn quitting_with_a_download_running_asks_first_and_closes_nothing() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let window = browser.main_window();
    browser.set_downloading(downloading(slot_of(&browser, ids[0]).unwrap(), false));
    let held = Effect::ConfirmClose {
        window,
        tab: None,
        ending: Ending::Quit,
        downloads: 1,
    };

    assert_eq!(browser.ending(window, None), Some(Ending::Quit));
    assert_eq!(browser.close_held(window, None), Some(held.clone()));
    assert_eq!(browser.close_window(window), [held]);
    assert!(browser.tab(ids[0]).is_ok(), "nothing was closed");
    browser.set_downloading(Downloading::default());
    assert_eq!(browser.close_held(window, None), None);
}

#[test]
fn closing_the_last_private_tab_with_a_private_download_asks_first() {
    let mut browser = Browser::new(2);
    browser.open_tab("https://public.test", true);
    let (private, _) = browser.open_private_window("https://secret.test");
    let window = browser.window_of(private).unwrap();
    browser.set_downloading(downloading(slot_of(&browser, private).unwrap(), true));

    let effects = browser.close_tab(private, HOME).unwrap();

    assert_eq!(
        effects,
        [Effect::ConfirmClose {
            window,
            tab: Some(private),
            ending: Ending::PrivateSession,
            downloads: 1
        }]
    );
    assert!(browser.tab(private).is_ok(), "nothing was closed");
    assert!(matches!(
        browser.close_window(window)[..],
        [Effect::ConfirmClose { tab: None, .. }]
    ));
}

#[test]
fn closes_that_cancel_no_download_are_not_held() {
    let mut browser = Browser::new(3);
    let (public, _) = browser.open_tab("https://public.test", true);
    let (first, _) = browser.open_private_window("https://one.test");
    let (second, _) = browser.open_private_window("https://two.test");
    let public_slot = slot_of(&browser, public).unwrap();
    browser.set_downloading(downloading(public_slot, false));

    // Another private window keeps the session.
    assert_eq!(browser.ending(browser.window_of(first).unwrap(), None), None);
    // An ordinary download survives the private session.
    assert_eq!(browser.close_held(browser.window_of(second).unwrap(), None), None);
    browser.close_window(browser.window_of(first).unwrap());
    assert_eq!(
        browser.ending(browser.window_of(second).unwrap(), Some(second)),
        Some(Ending::PrivateSession)
    );
    assert_eq!(
        browser.close_held(browser.window_of(second).unwrap(), Some(second)),
        None
    );
}

// -- fullscreen ---------------------------------------------------------

fn fullscreen_of(browser: &Browser, window: WindowId) -> Option<TabId> {
    browser.state_in(window).unwrap().fullscreen
}

#[test]
fn a_shown_page_entering_fullscreen_takes_its_window_fullscreen() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let window = browser.main_window();
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.report_fullscreen(slot, true);

    assert_eq!(fullscreen_of(&browser, window), Some(ids[0]));
    assert_eq!(
        effects,
        vec![Effect::SetFullscreen {
            window,
            fullscreen: true
        }]
    );
    assert!(
        browser.report_fullscreen(slot, true).is_empty(),
        "a repeated report changes nothing"
    );
}

#[test]
fn a_page_leaving_fullscreen_restores_its_window() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let window = browser.main_window();
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.report_fullscreen(slot, true);

    let effects = browser.report_fullscreen(slot, false);

    assert_eq!(fullscreen_of(&browser, window), None);
    assert_eq!(
        effects,
        vec![Effect::SetFullscreen {
            window,
            fullscreen: false
        }]
    );
}

#[test]
fn selecting_another_tab_leaves_fullscreen_and_asks_the_page_to_exit() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    let window = browser.main_window();
    let slot = slot_of(&browser, ids[1]).unwrap();
    browser.report_fullscreen(slot, true);

    let effects = browser.select_tab(ids[0], 0).unwrap();

    assert_eq!(fullscreen_of(&browser, window), None);
    assert!(effects.contains(&Effect::SetFullscreen {
        window,
        fullscreen: false
    }));
    assert!(effects.contains(&Effect::ExitPageFullscreen { slot }));
    // The page's own report that it left arrives afterwards and changes nothing.
    assert!(browser.report_fullscreen(slot, false).is_empty());
}

#[test]
fn selecting_the_other_tab_of_a_split_pair_leaves_fullscreen() {
    let (mut browser, _) = browser_with(2, &["https://left.test"]);
    let (window, left, right) = split_pair(&mut browser);
    let slot = slot_of(&browser, right).unwrap();
    browser.report_fullscreen(slot, true);

    let effects = browser.select_tab(left, 0).unwrap();

    assert_eq!(fullscreen_of(&browser, window), None);
    let leave = effects.iter().position(|effect| {
        *effect
            == Effect::SetFullscreen {
                window,
                fullscreen: false,
            }
    });
    let ask = effects
        .iter()
        .position(|effect| *effect == Effect::ExitPageFullscreen { slot });
    assert!(
        leave.unwrap() < ask.unwrap(),
        "the window leaves before the page is asked"
    );
}

#[test]
fn selecting_the_fullscreen_tab_again_keeps_fullscreen() {
    let (mut browser, _) = browser_with(2, &["https://left.test"]);
    let (window, _, right) = split_pair(&mut browser);
    browser.report_fullscreen(slot_of(&browser, right).unwrap(), true);

    browser.select_tab(right, 0).unwrap();

    assert_eq!(fullscreen_of(&browser, window), Some(right));
}

#[test]
fn closing_the_fullscreen_tab_restores_its_window() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    let window = browser.main_window();
    browser.report_fullscreen(slot_of(&browser, ids[1]).unwrap(), true);

    let effects = browser.close_tab(ids[1], HOME).unwrap();

    assert_eq!(fullscreen_of(&browser, window), None);
    assert!(effects.contains(&Effect::SetFullscreen {
        window,
        fullscreen: false
    }));
}

#[test]
fn a_crashed_fullscreen_page_restores_its_window() {
    let (mut browser, ids) = browser_with(2, &["https://a.test"]);
    let window = browser.main_window();
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.report_fullscreen(slot, true);

    let effects = browser.page_crashed(slot, ids[0], CrashCause::Unknown);

    assert_eq!(fullscreen_of(&browser, window), None);
    assert!(effects.contains(&Effect::SetFullscreen {
        window,
        fullscreen: false
    }));
    assert!(!effects.contains(&Effect::ExitPageFullscreen { slot }));
}

#[test]
fn a_hidden_page_entering_fullscreen_is_asked_to_exit() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    let window = browser.main_window();
    let hidden = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.report_fullscreen(hidden, true);

    assert_eq!(fullscreen_of(&browser, window), None);
    assert_eq!(effects, vec![Effect::ExitPageFullscreen { slot: hidden }]);
}

#[test]
fn an_unknown_slot_entering_fullscreen_changes_nothing() {
    let (mut browser, _) = browser_with(2, &["https://a.test"]);
    assert!(browser.report_fullscreen(SlotId(9), true).is_empty());
}

fn request(id: u64, kind: PermissionKind, origin: &str) -> PermissionRequest {
    PermissionRequest {
        id: PermissionRequestId(id),
        kind,
        origin: origin.into(),
    }
}

fn granted(effects: &[Effect]) -> Vec<(PermissionRequestId, bool)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::AnswerPermission { id, allow } => Some((*id, *allow)),
            _ => None,
        })
        .collect()
}

fn stored(origin: &str, kind: PermissionKind, state: PermissionState) -> SitePermission {
    SitePermission {
        origin: origin.into(),
        kind,
        state,
    }
}

fn shown_permission(browser: &Browser, id: TabId) -> Option<PermissionRequestId> {
    browser.tab(id).unwrap().permission.as_ref().map(|shown| shown.id)
}

#[test]
fn a_permission_request_waits_on_its_tab_and_others_queue_behind_it() {
    let (mut browser, id, slot) = loaded("https://a.test");

    let first = browser.request_permission(slot, request(1, PermissionKind::Camera, "https://a.test"));
    let second = browser.request_permission(slot, request(2, PermissionKind::Location, "https://a.test"));

    assert!(granted(&first).is_empty() && granted(&second).is_empty());
    assert_eq!(shown_permission(&browser, id), Some(PermissionRequestId(1)));

    let (effects, _) = browser
        .answer_permission(id, PermissionRequestId(1), PermissionDecision::Allow)
        .unwrap();
    assert_eq!(granted(&effects), vec![(PermissionRequestId(1), true)]);
    assert_eq!(shown_permission(&browser, id), Some(PermissionRequestId(2)));
}

#[test]
fn a_stored_decision_answers_a_request_without_asking() {
    let (browser, id, slot) = loaded("https://a.test");
    let mut browser = browser.with_site_permissions(vec![
        stored("https://a.test", PermissionKind::Camera, PermissionState::Allow),
        stored("https://a.test", PermissionKind::Location, PermissionState::Block),
    ]);

    let camera = browser.request_permission(slot, request(1, PermissionKind::Camera, "https://a.test"));
    let location = browser.request_permission(slot, request(2, PermissionKind::Location, "https://a.test"));
    let other = browser.request_permission(slot, request(3, PermissionKind::Camera, "http://a.test"));

    assert_eq!(granted(&camera), vec![(PermissionRequestId(1), true)]);
    assert_eq!(granted(&location), vec![(PermissionRequestId(2), false)]);
    assert!(granted(&other).is_empty(), "http is a different origin from https");
    assert_eq!(shown_permission(&browser, id), Some(PermissionRequestId(3)));
}

#[test]
fn allow_and_block_return_the_decision_to_store_and_answer_its_duplicates() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.request_permission(slot, request(1, PermissionKind::Microphone, "https://a.test"));
    browser.request_permission(slot, request(2, PermissionKind::Microphone, "https://a.test"));
    browser.request_permission(slot, request(3, PermissionKind::Camera, "https://a.test"));

    let (effects, decided) = browser
        .answer_permission(id, PermissionRequestId(1), PermissionDecision::Block)
        .unwrap();

    assert_eq!(
        granted(&effects),
        vec![(PermissionRequestId(1), false), (PermissionRequestId(2), false)]
    );
    assert_eq!(
        decided,
        Some(stored(
            "https://a.test",
            PermissionKind::Microphone,
            PermissionState::Block
        ))
    );
    assert_eq!(shown_permission(&browser, id), Some(PermissionRequestId(3)));
}

#[test]
fn a_dismissed_request_is_denied_unstored_and_not_asked_again() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.request_permission(slot, request(1, PermissionKind::Notifications, "https://a.test"));

    let (effects, decided) = browser
        .answer_permission(id, PermissionRequestId(1), PermissionDecision::Dismiss)
        .unwrap();
    let again = browser.request_permission(slot, request(2, PermissionKind::Notifications, "https://a.test"));

    assert_eq!(granted(&effects), vec![(PermissionRequestId(1), false)]);
    assert_eq!(decided, None);
    assert_eq!(granted(&again), vec![(PermissionRequestId(2), false)]);
    assert_eq!(shown_permission(&browser, id), None);
}

#[test]
fn a_late_answer_to_a_permission_request_that_is_gone_does_nothing() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.request_permission(slot, request(2, PermissionKind::Camera, "https://a.test"));

    let (effects, decided) = browser
        .answer_permission(id, PermissionRequestId(1), PermissionDecision::Allow)
        .unwrap();

    assert!(effects.is_empty());
    assert_eq!(decided, None);
    assert_eq!(shown_permission(&browser, id), Some(PermissionRequestId(2)));
}

#[test]
fn a_page_going_away_denies_its_permission_requests() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.request_permission(slot, request(1, PermissionKind::Camera, "https://a.test"));
    browser.request_permission(slot, request(2, PermissionKind::Location, "https://a.test"));

    let navigated = browser.navigate(id, "https://b.test").unwrap();
    assert_eq!(
        granted(&navigated),
        vec![(PermissionRequestId(1), false), (PermissionRequestId(2), false)]
    );
    assert_eq!(shown_permission(&browser, id), None);

    browser.request_permission(slot, request(3, PermissionKind::Camera, "https://b.test"));
    let closed = browser.close_tab(id, HOME).unwrap();
    assert_eq!(granted(&closed), vec![(PermissionRequestId(3), false)]);
}

#[test]
fn a_permission_request_from_a_slot_no_tab_owns_is_denied() {
    let mut browser = Browser::new(1);

    let effects = browser.request_permission(SlotId(0), request(1, PermissionKind::Camera, "https://a.test"));

    assert_eq!(granted(&effects), vec![(PermissionRequestId(1), false)]);
}

#[test]
fn a_private_window_honours_a_stored_block_but_not_a_stored_allow_and_stores_nothing() {
    let mut browser = Browser::new(2).with_site_permissions(vec![
        stored("https://a.test", PermissionKind::Camera, PermissionState::Allow),
        stored("https://a.test", PermissionKind::Location, PermissionState::Block),
    ]);
    let (id, _) = browser.open_private_window("https://a.test");
    browser.slots_created(browser.slot_ids());
    let slot = slot_of(&browser, id).unwrap();
    browser.report_page(slot, &[commit("https://a.test", NavigationKind::Push)], None);

    let location = browser.request_permission(slot, request(1, PermissionKind::Location, "https://a.test"));
    let camera = browser.request_permission(slot, request(2, PermissionKind::Camera, "https://a.test"));
    assert_eq!(granted(&location), vec![(PermissionRequestId(1), false)]);
    assert!(granted(&camera).is_empty(), "a stored Allow still asks");

    let (_, decided) = browser
        .answer_permission(id, PermissionRequestId(2), PermissionDecision::Allow)
        .unwrap();
    assert_eq!(decided, None);
    let again = browser.request_permission(slot, request(3, PermissionKind::Camera, "https://a.test"));
    assert_eq!(granted(&again), vec![(PermissionRequestId(3), true)]);
}

#[test]
fn a_request_from_an_origin_the_page_has_left_is_withdrawn() {
    let (mut browser, id, slot) = loaded("https://a.test");
    browser.request_permission(slot, request(1, PermissionKind::Camera, "https://a.test"));
    browser.request_permission(slot, request(2, PermissionKind::Camera, "https://b.test"));

    // The page navigates itself, so nothing released the request on the way.
    browser.report_page(slot, &[commit("https://b.test/next", NavigationKind::Push)], None);
    assert!(browser.has_stale_permissions(slot));
    let effects = browser.withdraw_stale_permissions(slot);

    assert_eq!(granted(&effects), vec![(PermissionRequestId(1), false)]);
    assert_eq!(shown_permission(&browser, id), Some(PermissionRequestId(2)));
    assert!(!browser.has_stale_permissions(slot));
}
