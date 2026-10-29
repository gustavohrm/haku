use super::*;
use crate::browser::Effect;
use crate::chrome::{Layout, PaneLayout};
use crate::state::AppState;
use crate::storage::{HistoryDb, Paths, Settings};
use crate::webview::Viewport;

fn app_state(browser: Browser) -> AppState {
    AppState::new(
        browser,
        Settings::default(),
        HistoryDb::open_or_in_memory(std::path::Path::new(":memory:")),
        Paths::under(&std::env::temp_dir().join("haku-session-tests")),
    )
}

fn measured_layout(browser: &Browser, window: WindowId) -> Layout {
    let state = browser.state_in(window).unwrap();
    Layout {
        revision: state.layout_revision,
        viewports: state
            .viewports
            .iter()
            .enumerate()
            .map(|(index, pane)| PaneLayout {
                id: pane.id.clone(),
                rect: Viewport {
                    x: index as f64 * 405.0,
                    y: 80.0,
                    width: 400.0,
                    height: 600.0,
                },
                covered: true,
                radius: 8.0,
            })
            .collect(),
        overlays: Vec::new(),
    }
}

fn accept_layout(state: &AppState, browser: &mut Browser, window: WindowId) -> Vec<Effect> {
    assert!(state
        .set_layout(browser, window, &measured_layout(browser, window))
        .unwrap());
    browser.reconcile()
}

#[test]
fn restored_windows_realize_only_their_shown_tabs_after_matching_geometry() {
    for active in [0, 1, 4] {
        let mut session = with_splits(
            &[
                "https://a.test",
                "https://b.test",
                "https://c.test",
                "https://d.test",
                "https://single.test",
            ],
            Some(active),
            serde_json::json!([{ "tabs": [0, 1], "ratio": 0.3 }, { "tabs": [2, 3], "ratio": 0.7 }]),
        );
        session.windows.push(window_of(&["https://other.test"], Some(0)));
        let state = app_state(restore(&session, 1));
        let mut browser = state.browser.write().unwrap();
        let windows = browser.window_ids();
        let first = browser.state_in(windows[0]).unwrap();
        assert!(browser.reconcile().is_empty());
        assert!(browser
            .tick(1, crate::model::Pressure::Normal, HashMap::new())
            .is_empty());
        assert!(browser.slots().is_empty());
        let mut stale = measured_layout(&browser, windows[0]);
        stale.revision -= 1;
        assert!(!state.set_layout(&mut browser, windows[0], &stale).unwrap());
        assert!(browser.reconcile().is_empty());
        let mut incomplete = measured_layout(&browser, windows[0]);
        incomplete.viewports.pop();
        assert!(state.set_layout(&mut browser, windows[0], &incomplete).is_err());
        assert!(browser.reconcile().is_empty());

        let effects = accept_layout(&state, &mut browser, windows[0]);
        let loaded: Vec<_> = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::EnsureSlot { url, .. } => Some(url.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            loaded,
            if active == 4 {
                vec!["https://single.test"]
            } else {
                vec!["https://a.test", "https://b.test"]
            }
        );
        assert!(browser.state_in(windows[1]).unwrap().tabs[0].slot().is_none());
        assert!(browser.tab(first.tabs[2].id).unwrap().slot().is_none());
        assert!(browser.tab(first.tabs[3].id).unwrap().slot().is_none());
        assert!(browser
            .slot_viewports()
            .values()
            .all(|target| state.viewports.can_show(target)));
        let effects = accept_layout(&state, &mut browser, windows[1]);
        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect, Effect::EnsureSlot { .. }))
                .count(),
            1
        );
        assert!(browser.state_in(windows[1]).unwrap().tabs[0].slot().is_some());
    }
}

#[test]
fn legacy_shapes_and_wrong_split_list_types_restore_tabs_without_pairs() {
    let tab = serde_json::json!({ "url": "https://a.test", "title": "A", "fixed": true });
    for splits in [
        serde_json::json!(null),
        serde_json::json!(false),
        serde_json::json!(4),
        serde_json::json!("pairs"),
        serde_json::json!({ "tabs": [0, 1], "ratio": 0.5 }),
        serde_json::json!([]),
    ] {
        let session: Session = serde_json::from_value(
            serde_json::json!({ "windows": [{ "tabs": [tab], "active": 0, "splits": splits }] }),
        )
        .unwrap();
        let browser = restore(&session, 1);
        assert_eq!(browser.tabs()[0].url(), "https://a.test");
        assert!(browser.state().splits.is_empty());
        assert!(
            serde_json::to_value(Session::capture(&browser, &HashMap::new())).unwrap()["windows"][0]
                .get("splits")
                .is_none()
        );
    }
    for stored in [
        serde_json::json!({ "windows": [{ "tabs": [tab], "active": 0, "split": { "anything": "ignored" } }] }),
        serde_json::json!({ "tabs": [tab], "active": 0, "split": { "anything": "ignored" } }),
    ] {
        let session = serde_json::from_value(stored).unwrap();
        let state = restore(&session, 1).state();
        assert_eq!(state.tabs.len(), 1);
        assert_eq!(state.tabs[0].url(), "https://a.test");
        assert!(state.tabs[0].fixed);
        assert_eq!(state.active, Some(state.tabs[0].id));
        assert!(state.splits.is_empty());
    }
}

#[test]
fn mixed_internal_and_external_pairs_realize_only_external_members() {
    for urls in [
        ["haku://settings", "https://a.test"],
        ["https://a.test", "haku://history"],
        ["haku://settings", "haku://history"],
    ] {
        let session = with_splits(&urls, Some(1), serde_json::json!([{ "tabs": [0, 1], "ratio": 0.4 }]));
        let state = app_state(restore(&session, 1));
        let mut browser = state.browser.write().unwrap();
        let window = browser.main_window();
        assert!(browser.tabs().iter().all(|tab| tab.slot().is_none()));
        assert_eq!(browser.state().viewports.len(), 2);
        let effects = accept_layout(&state, &mut browser, window);
        let external = urls.iter().filter(|url| url.starts_with("https:")).count();
        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect, Effect::EnsureSlot { .. }))
                .count(),
            external
        );
        assert_eq!(browser.state().live_count, external);
        for tab in browser.tabs() {
            assert_eq!(tab.slot().is_none(), tab.is_internal());
        }
    }
}

#[test]
fn fixed_background_pair_members_keep_existing_loaded_policy() {
    let mut session = with_splits(
        &["https://fixed.test", "https://partner.test", "haku://settings"],
        Some(2),
        serde_json::json!([{ "tabs": [0, 1], "ratio": 0.4 }]),
    );
    session.windows[0].tabs[0].fixed = true;
    let state = app_state(restore(&session, 1));
    let mut browser = state.browser.write().unwrap();
    let window = browser.main_window();
    assert!(browser.reconcile().is_empty());
    let effects = accept_layout(&state, &mut browser, window);
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::EnsureSlot { .. }))
            .count(),
        1
    );
    assert!(browser.tabs()[0].slot().is_some());
    assert!(browser.tabs()[1].slot().is_none());
    let captured = Session::capture(&browser, &HashMap::new());
    assert!(captured.windows[0].tabs[0].fixed);
    assert_eq!(captured.windows[0].splits.len(), 1);
}

#[test]
fn crashed_pair_capture_restores_as_discarded_with_membership_and_selection() {
    let session = with_splits(
        &["https://healthy.test", "https://crashed.test"],
        Some(1),
        serde_json::json!([{ "tabs": [0, 1], "ratio": 0.6 }]),
    );
    let state = app_state(restore(&session, 1));
    let mut browser = state.browser.write().unwrap();
    let window = browser.main_window();
    accept_layout(&state, &mut browser, window);
    let crashed = browser.tabs()[1].id;
    let slot = browser.tab(crashed).unwrap().slot().unwrap();
    browser.page_crashed(slot, crashed, crate::model::CrashCause::Ended);
    assert!(browser.tab(crashed).unwrap().is_crashed());
    let stored = serde_json::to_value(Session::capture(&browser, &HashMap::new())).unwrap();
    let restored = restore(&serde_json::from_value(stored).unwrap(), 1);
    assert_eq!(restored.state().splits.len(), 1);
    assert_eq!(restored.state().splits[0].ratio, 0.6);
    assert_eq!(restored.state().viewports.len(), 2);
    assert_eq!(restored.active(), Some(restored.tabs()[1].id));
    assert!(restored
        .tabs()
        .iter()
        .all(|tab| tab.presence == crate::model::TabPresence::Discarded && tab.slot().is_none()));
}

#[test]
fn restoring_pair_pages_is_not_a_visit_but_later_navigation_is() {
    let session = with_splits(
        &["https://a.test", "https://b.test"],
        Some(1),
        serde_json::json!([{ "tabs": [0, 1], "ratio": 0.6 }]),
    );
    let state = app_state(restore(&session, 1));
    let mut browser = state.browser.write().unwrap();
    let window = browser.main_window();
    accept_layout(&state, &mut browser, window);
    for tab in browser.tabs().to_vec() {
        let report = browser
            .report_page(
                tab.slot().unwrap(),
                &[crate::model::Commit {
                    url: tab.url().into(),
                    kind: crate::model::NavigationKind::Push,
                }],
                None,
            )
            .unwrap();
        assert!(!report.visited);
    }
    let selected = browser.active().unwrap();
    browser.navigate(selected, "https://next.test").unwrap();
    let slot = browser.tab(selected).unwrap().slot().unwrap();
    assert!(
        browser
            .report_page(
                slot,
                &[crate::model::Commit {
                    url: "https://next.test".into(),
                    kind: crate::model::NavigationKind::Push,
                }],
                None
            )
            .unwrap()
            .visited
    );
}

#[test]
fn filtered_empty_windows_and_home_fallback_do_not_inherit_pairs() {
    let mut session = with_splits(
        &["javascript:bad", "not a url"],
        Some(1),
        serde_json::json!([{ "tabs": [0, 1], "ratio": 0.5 }]),
    );
    session.windows.push(window_of(&[], Some(0)));
    let (browser, windows) = session.restore(1, "haku://settings");
    assert_eq!(windows.len(), 1);
    assert_eq!(browser.tabs()[0].url(), "haku://settings");
    assert_eq!(browser.active(), Some(browser.tabs()[0].id));
    assert!(browser.state().splits.is_empty());
    session.windows.push(window_of(&["https://surviving.test"], None));
    let (browser, windows) = session.restore(1, "haku://settings");
    assert_eq!(windows.len(), 1);
    assert_eq!(browser.tabs()[0].url(), "https://surviving.test");
    assert!(browser.state().splits.is_empty());
}

fn with_splits(urls: &[&str], active: Option<usize>, splits: serde_json::Value) -> Session {
    let mut stored = serde_json::to_value(session_of(urls, active)).unwrap();
    stored["windows"][0]["splits"] = splits;
    serde_json::from_value(stored).unwrap()
}

#[test]
fn malformed_split_entries_drop_only_that_pair_and_keep_the_selected_tab() {
    for invalid in [
        serde_json::json!(null),
        serde_json::json!(false),
        serde_json::json!("pair"),
        serde_json::json!([]),
        serde_json::json!({}),
        serde_json::json!({ "tabs": [0, 1] }),
        serde_json::json!({ "tabs": [0, 1], "ratio": "0.5" }),
        serde_json::json!({ "tabs": [0, 1], "ratio": null }),
        serde_json::json!({ "tabs": "0,1", "ratio": 0.5 }),
        serde_json::json!({ "tabs": [0], "ratio": 0.5 }),
        serde_json::json!({ "tabs": [0, 1, 2], "ratio": 0.5 }),
        serde_json::json!({ "tabs": [-1, 0], "ratio": 0.5 }),
        serde_json::json!({ "tabs": [0, 1.5], "ratio": 0.5 }),
        serde_json::json!({ "tabs": [0, "1"], "ratio": 0.5 }),
        serde_json::json!({ "tabs": [0, 0], "ratio": 0.5 }),
        serde_json::json!({ "tabs": [1, 0], "ratio": 0.5 }),
        serde_json::json!({ "tabs": [0, 2], "ratio": 0.5 }),
        serde_json::json!({ "tabs": [4, 5], "ratio": 0.5 }),
        serde_json::json!({ "tabs": [usize::MAX, 0], "ratio": 0.5 }),
        serde_json::json!({ "tabs": [0, 1], "ratio": 0.199 }),
        serde_json::json!({ "tabs": [0, 1], "ratio": 0.801 }),
    ] {
        let mut stored = serde_json::to_value(session_of(
            &["https://a.test", "https://b.test", "https://c.test", "https://d.test"],
            Some(1),
        ))
        .unwrap();
        stored["windows"][0]["splits"] = serde_json::json!([
            invalid, { "tabs": [2, 3], "ratio": 0.6 }
        ]);
        let session = serde_json::from_value(stored.clone()).unwrap_or_else(|error| panic!("{stored}: {error}"));
        let state = restore(&session, 1).state();
        assert_eq!(state.splits.len(), 1, "{stored}");
        assert_eq!(state.splits[0].tabs, [state.tabs[2].id, state.tabs[3].id], "{stored}");
        assert_eq!(state.active, Some(state.tabs[1].id), "{stored}");
        assert_eq!(state.viewports.len(), 1, "{stored}");
    }
}

#[test]
fn the_first_valid_split_claims_its_members() {
    let session = with_splits(
        &[
            "https://a.test",
            "https://b.test",
            "https://c.test",
            "https://d.test",
            "https://e.test",
            "https://f.test",
        ],
        Some(2),
        serde_json::json!([
            { "tabs": [0, 1], "ratio": 0.1 },
            { "tabs": [1, 2], "ratio": 0.3 },
            { "tabs": [1, 2], "ratio": 0.4 },
            { "tabs": [0, 1], "ratio": 0.5 },
            { "tabs": [2, 3], "ratio": 0.6 },
            { "tabs": [4, 5], "ratio": 0.7 }
        ]),
    );
    let state = restore(&session, 1).state();
    assert_eq!(state.splits.len(), 2);
    assert_eq!(state.splits[0].tabs, [state.tabs[1].id, state.tabs[2].id]);
    assert_eq!(state.splits[0].ratio, 0.3);
    assert_eq!(state.splits[1].tabs, [state.tabs[4].id, state.tabs[5].id]);
    assert_eq!(state.splits[1].ratio, 0.7);
}

#[test]
fn url_filtering_remaps_pairs_without_substituting_a_filtered_member() {
    for active in [Some(1), Some(2), Some(7), Some(99), None] {
        let session = with_splits(
            &[
                "javascript:bad",
                "https://a.test",
                "file:///a",
                "not a url",
                "https://b.test",
                "haku://settings",
                "https://c.test",
                "javascript:bad",
            ],
            active,
            serde_json::json!([
                { "tabs": [0, 1], "ratio": 0.5 },
                { "tabs": [2, 3], "ratio": 0.5 },
                { "tabs": [4, 5], "ratio": 0.7 },
                { "tabs": [6, 7], "ratio": 0.5 }
            ]),
        );
        let state = restore(&session, 1).state();
        assert_eq!(state.tabs.len(), 5);
        assert_eq!(state.splits.len(), 1);
        assert_eq!(state.splits[0].tabs, [state.tabs[2].id, state.tabs[3].id]);
        let selected = match active {
            Some(2) => 1,
            Some(7) => 4,
            _ => 0,
        };
        assert_eq!(state.active, Some(state.tabs[selected].id));
        assert_eq!(state.viewports.len(), 1);
    }
}

#[test]
fn filtering_cannot_make_a_nonadjacent_stored_pair_valid() {
    let session = with_splits(
        &["https://a.test", "javascript:bad", "https://b.test", "https://c.test"],
        Some(2),
        serde_json::json!([{ "tabs": [0, 2], "ratio": 0.5 }, { "tabs": [2, 3], "ratio": 0.6 }]),
    );
    let state = restore(&session, 1).state();
    assert_eq!(state.splits.len(), 1);
    assert_eq!(state.splits[0].tabs, [state.tabs[1].id, state.tabs[2].id]);
    assert_eq!(state.active, Some(state.tabs[1].id));
    assert_eq!(state.viewports.len(), 2);
}

#[test]
fn nonfinite_ratios_drop_only_the_invalid_pair() {
    for ratio in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut session = with_splits(
            &["https://a.test", "https://b.test", "https://c.test", "https://d.test"],
            Some(0),
            serde_json::json!([{ "tabs": [0, 1], "ratio": 0.5 }, { "tabs": [2, 3], "ratio": 0.6 }]),
        );
        session.windows[0].splits[0].ratio = ratio;
        let state = restore(&session, 1).state();
        assert_eq!(state.splits.len(), 1);
        assert_eq!(state.splits[0].tabs, [state.tabs[2].id, state.tabs[3].id]);
        assert_eq!(state.active, Some(state.tabs[0].id));
    }
}

#[test]
fn restoring_either_pair_member_shows_the_pair_and_focuses_that_member() {
    for active in [0, 1] {
        let session = with_splits(
            &["https://a.test", "https://b.test", "https://c.test"],
            Some(active),
            serde_json::json!([{ "tabs": [0, 1], "ratio": 0.3 }]),
        );
        let browser = restore(&session, 1);
        let state = browser.state();

        assert_eq!(state.splits.len(), 1);
        assert_eq!(state.splits[0].tabs, [state.tabs[0].id, state.tabs[1].id]);
        assert_eq!(state.splits[0].ratio, 0.3);
        assert_eq!(
            state.viewports.iter().map(|pane| pane.tab).collect::<Vec<_>>(),
            state.splits[0].tabs
        );
        assert_ne!(state.viewports[0].id, state.viewports[1].id);
        assert_eq!(state.active, Some(state.tabs[active].id));
        assert_eq!(state.focused_viewport, state.viewports[active].id);
        assert_eq!(state.live_count, 0);
        assert!(state
            .tabs
            .iter()
            .all(|tab| tab.presence == crate::model::TabPresence::Discarded && tab.slot().is_none()));
    }
}

#[test]
fn capture_round_trips_all_pairs_ratios_and_windows_with_an_active_single() {
    let mut browser = restore(
        &Session {
            windows: vec![
                window_of(
                    &[
                        "https://a.test",
                        "https://b.test",
                        "https://c.test",
                        "https://d.test",
                        "haku://settings",
                    ],
                    Some(0),
                ),
                window_of(&["https://e.test", "https://f.test"], Some(0)),
            ],
            ..Session::default()
        },
        1,
    );
    let windows = browser.window_ids();
    for window in &windows {
        let tabs = browser.state_in(*window).unwrap().tabs;
        browser.split_tab(*window, tabs[1].id).unwrap();
        browser.set_split_ratio(*window, tabs[0].id, 0.2).unwrap();
        if tabs.len() == 5 {
            browser.select_tab(tabs[2].id, 0).unwrap();
            browser.split_tab(*window, tabs[3].id).unwrap();
            browser.set_split_ratio(*window, tabs[2].id, 0.8).unwrap();
            browser.select_tab(tabs[4].id, 0).unwrap();
        }
    }
    let bounds = WindowBounds {
        x: 12.0,
        y: 30.0,
        width: 800.0,
        height: 600.0,
        maximized: false,
    };
    let captured = Session::capture(&browser, &HashMap::from([(windows[0], bounds)]));
    let stored = serde_json::to_value(&captured).unwrap();

    assert_eq!(
        stored["windows"][0]["splits"],
        serde_json::json!([
            { "tabs": [0, 1], "ratio": 0.2 }, { "tabs": [2, 3], "ratio": 0.8 }
        ])
    );
    assert_eq!(
        stored["windows"][1]["splits"],
        serde_json::json!([{ "tabs": [0, 1], "ratio": 0.2 }])
    );
    let read: Session = serde_json::from_value(stored.clone()).unwrap();
    let restored = restore(&read, 1);
    let first = restored.state_in(restored.window_ids()[0]).unwrap();
    assert_eq!(first.viewports.len(), 1);
    assert_eq!(first.viewports[0].tab, first.tabs[4].id);
    assert_eq!(first.splits.len(), 2);
    assert_eq!(first.live_count, 0);
    assert_eq!(
        serde_json::to_value(Session::capture(
            &restored,
            &HashMap::from([(restored.window_ids()[0], bounds)])
        ))
        .unwrap(),
        stored
    );
}

fn window_of(urls: &[&str], active: Option<usize>) -> SessionWindow {
    SessionWindow {
        tabs: urls
            .iter()
            .map(|url| SessionTab {
                url: (*url).to_string(),
                title: (*url).to_string(),
                fixed: false,
            })
            .collect(),
        active,
        bounds: None,
        splits: Vec::new(),
    }
}

fn session_of(urls: &[&str], active: Option<usize>) -> Session {
    Session {
        windows: vec![window_of(urls, active)],
        ..Session::default()
    }
}

fn restore(session: &Session, capacity: usize) -> Browser {
    session.restore(capacity, "haku://new-tab").0
}

#[test]
fn an_empty_session_restores_to_the_home_page() {
    let browser = restore(&Session::default(), 1);

    assert_eq!(browser.tabs().len(), 1);
    assert_eq!(browser.tabs()[0].url(), "haku://new-tab");
}

#[test]
fn restoring_reopens_every_tab_in_order() {
    let browser = restore(&session_of(&["https://a.test", "https://b.test"], Some(0)), 1);

    let urls: Vec<&str> = browser.tabs().iter().map(|tab| tab.url()).collect();
    assert_eq!(urls, vec!["https://a.test", "https://b.test"]);
}

#[test]
fn restoring_reactivates_the_tab_that_was_active() {
    let browser = restore(&session_of(&["https://a.test", "https://b.test"], Some(1)), 1);

    assert_eq!(browser.active(), browser.tabs().get(1).map(|tab| tab.id));
}

#[test]
fn an_out_of_range_active_index_falls_back_to_the_first_tab() {
    let browser = restore(&session_of(&["https://a.test"], Some(9)), 1);

    assert_eq!(browser.active(), browser.tabs().first().map(|tab| tab.id));
}

#[test]
fn pinned_tabs_are_still_pinned_after_a_restart() {
    let mut session = session_of(&["https://a.test", "https://b.test"], Some(1));
    session.windows[0].tabs[0].fixed = true;

    let browser = restore(&session, 1);

    assert!(browser.tabs()[0].fixed);
}

#[test]
fn a_restored_browser_holds_no_webviews_until_it_is_reconciled() {
    // Restoring through the ordinary open and select path would mark tabs
    // live and record which slot holds which URL, while producing effects
    // that nobody applies. Reconciling against the first reported layout
    // would then find nothing to do and the window would stay blank.
    let browser = restore(&session_of(&["https://a.test", "https://b.test"], Some(0)), 2);

    assert_eq!(browser.state().live_count, 0);
    assert!(browser.tabs().iter().all(|tab| tab.slot().is_none()));
}

#[test]
fn reconciling_a_restored_session_loads_the_active_tab() {
    let mut browser = restore(&session_of(&["https://a.test", "https://b.test"], Some(1)), 1);

    browser.layout_ready(browser.main_window());
    let effects = browser.reconcile();

    assert!(effects.iter().any(|effect| matches!(
        effect,
        crate::browser::Effect::EnsureSlot { url, .. } if url == "https://b.test"
    )));
}

#[test]
fn a_captured_session_restores_to_the_same_tabs() {
    let original = session_of(&["https://a.test", "https://b.test"], Some(1));
    let browser = restore(&original, 2);

    let captured = Session::capture(&browser, &HashMap::new());

    let urls: Vec<&str> = captured.windows[0].tabs.iter().map(|tab| tab.url.as_str()).collect();
    assert_eq!(urls, vec!["https://a.test", "https://b.test"]);
    assert_eq!(captured.windows[0].active, Some(1));
}

#[test]
fn every_window_is_restored_with_its_own_tabs_and_active_tab() {
    let session = Session {
        windows: vec![
            window_of(&["https://a.test", "https://b.test"], Some(1)),
            window_of(&["https://c.test"], Some(0)),
        ],
        ..Session::default()
    };

    let browser = restore(&session, 2);

    let windows = browser.window_ids();
    assert_eq!(windows.len(), 2);
    let first = browser.state_in(windows[0]).unwrap();
    let urls: Vec<&str> = first.tabs.iter().map(|tab| tab.url()).collect();
    assert_eq!(urls, vec!["https://a.test", "https://b.test"]);
    assert_eq!(first.active, Some(first.tabs[1].id));
    assert_eq!(browser.state_in(windows[1]).unwrap().tabs[0].url(), "https://c.test");
}

#[test]
fn the_window_used_last_is_the_one_in_use_after_a_restart() {
    let session = Session {
        windows: vec![
            window_of(&["https://a.test"], None),
            window_of(&["https://c.test"], None),
        ],
        ..Session::default()
    };

    let browser = restore(&session, 2);

    assert_eq!(browser.tab(browser.active().unwrap()).unwrap().url(), "https://c.test");
}

#[test]
fn a_session_stored_before_windows_existed_restores_as_one_window() {
    let stored = r#"{ "tabs": [{ "url": "https://a.test", "title": "A", "fixed": false }], "active": 0 }"#;
    let session: Session = serde_json::from_str(stored).unwrap();

    let browser = restore(&session, 1);

    assert_eq!(browser.window_ids().len(), 1);
    assert_eq!(browser.tabs()[0].url(), "https://a.test");
}

#[test]
fn the_old_shape_is_never_written_back() {
    let session = session_of(&["https://a.test"], Some(0));

    let written = serde_json::to_value(&session).unwrap();

    assert!(written.get("tabs").is_none());
    assert!(written.get("windows").is_some());
}

#[test]
fn a_captured_session_keeps_where_each_window_was() {
    let browser = restore(&session_of(&["https://a.test"], Some(0)), 1);
    let bounds = WindowBounds {
        x: 10.0,
        y: 20.0,
        width: 800.0,
        height: 600.0,
        maximized: false,
    };

    let captured = Session::capture(&browser, &HashMap::from([(browser.main_window(), bounds)]));

    assert_eq!(captured.windows[0].bounds, Some(bounds));
}

#[test]
fn popups_are_not_remembered() {
    let mut browser = restore(
        &with_splits(
            &["https://a.test", "https://b.test"],
            Some(0),
            serde_json::json!([{ "tabs": [0, 1], "ratio": 0.4 }]),
        ),
        2,
    );
    browser.layout_ready(browser.main_window());
    browser.reconcile();
    let opener = browser.tab(browser.active().unwrap()).unwrap().slot().unwrap();
    browser.open_from(
        opener,
        "https://login.test",
        crate::browser::Opening::Popup {
            request: crate::model::WindowRequestId(1),
            placement: crate::model::Placement::default(),
        },
    );

    let captured = Session::capture(&browser, &HashMap::new());

    assert_eq!(captured.windows.len(), 1);
    assert_eq!(captured.windows[0].tabs.len(), 2);
    assert_eq!(
        captured.windows[0].splits,
        vec![SessionSplit {
            tabs: [0, 1],
            ratio: 0.4
        }]
    );
}

#[test]
fn a_tab_with_an_address_haku_does_not_open_is_not_restored() {
    let windows = session_of(
        &["https://a.test", "javascript:alert(1)", "not a url", "haku://settings"],
        Some(3),
    )
    .windows_or_home("haku://new-tab");

    let urls: Vec<&str> = windows[0].tabs.iter().map(|tab| tab.url.as_str()).collect();
    assert_eq!(urls, vec!["https://a.test", "haku://settings"]);
    assert_eq!(windows[0].active, Some(1));
}

#[test]
fn an_active_index_past_the_tabs_still_falls_back_to_the_first_tab() {
    let windows = session_of(&["https://a.test", "javascript:alert(1)", "https://b.test"], Some(9))
        .windows_or_home("haku://new-tab");

    assert_eq!(windows[0].active, None);
}

#[test]
fn a_window_left_with_no_tab_it_can_open_is_not_restored() {
    let session = Session {
        windows: vec![
            window_of(&["javascript:alert(1)"], Some(0)),
            window_of(&["https://a.test"], Some(0)),
        ],
        ..Session::default()
    };

    let windows = session.windows_or_home("haku://new-tab");

    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].tabs[0].url, "https://a.test");
}
