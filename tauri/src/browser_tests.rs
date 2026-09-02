use super::*;

const SEARCH: &str = "https://duckduckgo.com/?q=";

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

    assert!(slot_of(&browser, ids[0]).is_none(), "the first tab gave up the only slot");
    assert_eq!(slot_of(&browser, ids[1]), Some(SlotId(0)));

    browser.select_tab(ids[0], 0).unwrap();
    assert_eq!(slot_of(&browser, ids[0]), Some(SlotId(0)));
    assert!(slot_of(&browser, ids[1]).is_none());
}

#[test]
fn selecting_a_suspended_tab_reloads_its_url_into_the_slot() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);

    let effects = browser.select_tab(ids[0], 0).unwrap();

    assert!(effects.contains(&Effect::EnsureSlot { slot: SlotId(0), url: "https://a.test".to_string() }));
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
    let (browser, ids) = browser_with(1, &["haku:settings"]);
    assert!(slot_of(&browser, ids[0]).is_none());
    assert!(browser.tab(ids[0]).unwrap().is_internal());
}

#[test]
fn navigating_from_the_web_to_an_internal_page_frees_the_slot() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    assert!(slot_of(&browser, ids[0]).is_some());

    let effects = browser.navigate(ids[0], "haku:settings").unwrap();

    assert!(slot_of(&browser, ids[0]).is_none());
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Blank { .. })));
}

#[test]
fn closing_the_active_tab_activates_its_right_neighbour() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test", "https://c.test"]);
    browser.select_tab(ids[1], 0).unwrap();
    browser.close_tab(ids[1]).unwrap();

    assert_eq!(browser.active(), Some(ids[2]));
}

#[test]
fn closing_the_last_tab_activates_the_one_before_it() {
    let (mut browser, ids) = browser_with(2, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[1], 0).unwrap();
    browser.close_tab(ids[1]).unwrap();

    assert_eq!(browser.active(), Some(ids[0]));
}

#[test]
fn closing_a_tab_blanks_the_webview_it_was_holding() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    let effects = browser.close_tab(ids[0]).unwrap();

    assert!(effects.contains(&Effect::Blank { slot }));
}

#[test]
fn closing_an_unknown_tab_reports_it_rather_than_failing_silently() {
    let (mut browser, _) = browser_with(1, &["https://a.test"]);
    assert!(matches!(browser.close_tab(TabId(404)), Err(HakuError::TabNotFound(_))));
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

#[test]
fn a_fixed_tab_that_has_gone_idle_gives_its_slot_back() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();
    browser.set_fixed(ids[0], true).unwrap();
    browser.select_tab(ids[1], 0).unwrap();
    browser.report_activity(slot_of(&browser, ids[0]).unwrap(), 1_000);

    let effects = browser.release_idle_fixed(1_000 + 60_000, 30_000);

    assert!(slot_of(&browser, ids[0]).is_none());
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Blank { .. })));
}

#[test]
fn a_fixed_tab_still_reporting_activity_keeps_its_slot() {
    let (mut browser, ids) = browser_with(1, &["https://a.test", "https://b.test"]);
    browser.select_tab(ids[0], 0).unwrap();
    browser.set_fixed(ids[0], true).unwrap();
    browser.select_tab(ids[1], 0).unwrap();
    browser.report_activity(slot_of(&browser, ids[0]).unwrap(), 1_000);

    browser.release_idle_fixed(1_010, 30_000);

    assert!(slot_of(&browser, ids[0]).is_some());
}

#[test]
fn the_active_tab_is_never_released_as_idle_even_when_fixed() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    browser.set_fixed(ids[0], true).unwrap();

    browser.release_idle_fixed(u64::MAX, 0);

    assert!(slot_of(&browser, ids[0]).is_some());
}

#[test]
fn a_page_reporting_a_redirect_replaces_the_entry_instead_of_adding_one() {
    let (mut browser, ids) = browser_with(1, &["https://a.test"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    browser.report_page(slot, Some("https://a.test/final".into()), Some("Final".into()));

    let tab = browser.tab(ids[0]).unwrap();
    assert_eq!(tab.history.entries().len(), 1);
    assert_eq!(tab.url(), "https://a.test/final");
    assert_eq!(tab.history.current().title, "Final");
}

#[test]
fn a_reported_page_supplies_a_favicon_from_the_site_root() {
    let (mut browser, ids) = browser_with(1, &["https://a.test/deep/page"]);
    let slot = slot_of(&browser, ids[0]).unwrap();

    browser.report_page(slot, Some("https://a.test/deep/page".into()), Some("A".into()));

    let favicon = browser.tab(ids[0]).unwrap().history.current().favicon.clone();
    assert_eq!(favicon.as_deref(), Some("https://a.test/favicon.ico"));
}

#[test]
fn a_favicon_is_only_derived_for_web_pages() {
    assert_eq!(favicon_for("haku:settings"), None);
    assert_eq!(favicon_for("https://a.test/x"), Some("https://a.test/favicon.ico".to_string()));
    assert_eq!(favicon_for("http://a.test"), Some("http://a.test/favicon.ico".to_string()));
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
    assert_eq!(resolve_target("haku:settings", SEARCH), "haku:settings");
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
    assert_eq!(resolve_target("what is node.js", SEARCH), format!("{SEARCH}what+is+node.js"));
}

#[test]
fn reserved_characters_in_a_search_are_percent_encoded() {
    assert_eq!(resolve_target("a&b=c", SEARCH), format!("{SEARCH}a%26b%3Dc"));
}

#[test]
fn an_empty_address_resolves_to_nothing() {
    assert_eq!(resolve_target("   ", SEARCH), "");
}
