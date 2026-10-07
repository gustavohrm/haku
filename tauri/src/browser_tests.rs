use super::*;
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
        url: "https://a.test".to_string()
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
        url: "https://spa.test/thread/42".to_string()
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
    let mut browser = Browser::restored(1, [("https://a.test".to_string(), false)], Some(0));
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

#[test]
fn smart_discarding_frees_a_tab_left_unshown_for_long() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Smart);
    browser.relieve(1_000, false);

    let effects = browser.relieve(1_000 + LONG_UNSHOWN_MS, false);

    assert_eq!(presence(&browser, ids[0]), TabPresence::Discarded);
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Blank { .. })));
    assert!(
        matches!(presence(&browser, ids[2]), TabPresence::Live { .. }),
        "the visible tab stays"
    );
}

#[test]
fn low_memory_frees_tabs_not_shown_recently_but_spares_recent_ones() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Smart);
    browser.relieve(1_000, false);
    browser.select_tab(ids[1], 1_000 + RECENTLY_SHOWN_MS).unwrap();
    browser.select_tab(ids[2], 1_000 + RECENTLY_SHOWN_MS).unwrap();

    browser.relieve(1_000 + RECENTLY_SHOWN_MS + 1, true);

    assert_eq!(presence(&browser, ids[0]), TabPresence::Discarded);
    assert!(matches!(presence(&browser, ids[1]), TabPresence::Frozen { .. }));
}

#[test]
fn smart_discarding_spares_a_tab_playing_audio() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Smart);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.report_audio(slot, true, 0);
    browser.relieve(1_000, false);

    browser.relieve(1_000 + LONG_UNSHOWN_MS, true);

    assert!(slot_of(&browser, ids[0]).is_some());
}

#[test]
fn discarding_never_leaves_old_tabs_loaded_even_when_memory_is_low() {
    let (mut browser, ids) = optimized(Policy::Smart, Policy::Never);
    browser.relieve(1_000, false);

    let effects = browser.relieve(1_000 + LONG_UNSHOWN_MS, true);

    assert!(effects.is_empty());
    assert!(slot_of(&browser, ids[0]).is_some());
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
fn smart_discarding_frees_an_audible_tab_when_freezing_always() {
    let mut browser = Browser::new(3).with_policies(Policy::Always, Policy::Smart);
    let (music, _) = browser.open_tab("https://music.test", true);
    let slot = slot_of(&browser, music).unwrap();
    browser.report_audio(slot, true, 0);
    browser.open_tab("https://b.test", true);
    browser.relieve(1_000, false);

    browser.relieve(1_000 + LONG_UNSHOWN_MS, false);

    assert_eq!(presence(&browser, music), TabPresence::Discarded);
}

#[test]
fn a_tab_that_falls_silent_is_timed_from_that_moment() {
    let (mut browser, ids) = optimized(Policy::Never, Policy::Smart);
    let slot = slot_of(&browser, ids[0]).unwrap();
    browser.report_audio(slot, true, 0);
    browser.relieve(1_000, false);
    browser.report_audio(slot, false, LONG_UNSHOWN_MS);

    browser.relieve(1_000 + LONG_UNSHOWN_MS, false);

    assert!(slot_of(&browser, ids[0]).is_some());
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

    let leave = Effect::Leave { slot, tab: ids[0] };
    assert!(position(&effects, &leave) < position(&effects, &Effect::Freeze { slot }));
}

#[test]
fn a_page_is_read_before_it_is_parked() {
    let (mut browser, ids) = optimized(Policy::Never, Policy::Always);
    browser.select_tab(ids[0], 0).unwrap();
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.select_tab(ids[1], 0).unwrap();

    let leave = Effect::Leave { slot, tab: ids[0] };
    assert!(position(&effects, &leave) < position(&effects, &Effect::Blank { slot }));
}

#[test]
fn an_evicted_page_is_read_before_another_tab_takes_its_slot() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    let (_, effects) = browser.open_tab("https://b.test", true);

    let leave = Effect::Leave { slot, tab: ids[0] };
    let ensure = Effect::EnsureSlot {
        slot,
        url: "https://b.test".into(),
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
