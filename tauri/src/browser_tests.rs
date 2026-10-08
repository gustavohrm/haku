use super::*;
use crate::model::page_state::INTERACTION_THRESHOLD;
use crate::model::Loss;

const SEARCH: &str = "https://duckduckgo.com/?q=";
const HOME: &str = "haku://new-tab";

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
    let mut browser = Browser::restored(1, [(vec![("https://a.test".to_string(), false)], Some(0))]);
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
    let (popup, _) = popup_from(&mut browser, ids[0]);
    let popup_window = browser.window_of(popup).unwrap();

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
    let second = browser.open_tab_in(window, "https://c.test", false).0;
    let slot = slot_of(&browser, tab).unwrap();

    let effects = browser.close_window(window);

    let destroyed = position(&effects, &Effect::Destroy { slot });
    let closed = position(&effects, &Effect::CloseWindow { window });
    assert!(destroyed < closed, "a webview goes before its window does");
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
