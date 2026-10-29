use super::*;
use crate::browser::ViewportAssignment;
use crate::chrome::{Layout, PaneLayout};

fn report() -> Layout {
    Layout {
        revision: 4,
        viewports: vec![PaneLayout {
            id: "1".into(),
            rect: Viewport {
                x: 4.0,
                y: 80.0,
                width: 900.0,
                height: 600.0,
            },
            covered: false,
            radius: 12.0,
        }],
        overlays: Vec::new(),
    }
}

fn target() -> ViewportTarget {
    ViewportTarget {
        window: WindowId(1),
        id: "1".into(),
        revision: 4,
    }
}

fn assignments() -> Vec<ViewportAssignment> {
    vec![ViewportAssignment {
        id: "1".into(),
        tab: TabId(1),
    }]
}

#[test]
fn selecting_another_tab_can_show_its_slot_without_a_new_layout() {
    let mut browser = crate::browser::Browser::new(2);
    let (first, _) = browser.open_tab("https://a.test", true);
    browser.open_tab("https://b.test", true);
    let current = browser.state();
    let mut layout = report();
    layout.revision = current.layout_revision;
    let viewports = Viewports::default();
    viewports
        .record(
            browser.main_window(),
            &layout,
            &current.viewports,
            current.layout_revision,
        )
        .unwrap();

    let effects = browser.select_tab(first, 0).unwrap();
    let slot = browser.tab(first).unwrap().slot().unwrap();
    assert!(effects.contains(&Effect::Show { slot }));
    assert!(viewports.can_show(&browser.slot_viewports()[&slot]));
    assert_eq!(browser.state().layout_revision, current.layout_revision);
}

#[test]
fn resized_bounds_reach_visible_background_and_fixed_slots_only_in_the_reporting_window() {
    let mut browser = crate::browser::Browser::new(4);
    let (fixed, _) = browser.open_tab("https://fixed.test", true);
    browser.set_fixed(fixed, true).unwrap();
    let (background, _) = browser.open_tab("https://background.test", true);
    let (visible, _) = browser.open_tab("https://visible.test", true);
    let window = browser.window_of(visible).unwrap();
    let (other, _) = browser.open_window("https://other.test");
    let current = browser.state_in(window).unwrap();
    let viewports = Viewports::default();
    let mut layout = report();
    layout.revision = current.layout_revision;
    viewports
        .record(window, &layout, &current.viewports, current.layout_revision)
        .unwrap();

    layout.viewports[0].rect.width = 1200.0;
    viewports
        .record(window, &layout, &current.viewports, current.layout_revision)
        .unwrap();
    let bounds = viewports.slot_bounds(&browser, window);
    assert_eq!(bounds.len(), 3);
    for tab in [fixed, background, visible] {
        let slot = browser.tab(tab).unwrap().slot().unwrap();
        assert!(bounds.contains(&(slot, layout.viewports[0].rect)));
    }
    let other_slot = browser.tab(other).unwrap().slot().unwrap();
    assert!(bounds.iter().all(|(slot, _)| *slot != other_slot));

    let measured = layout.viewports[0].rect;
    layout.viewports[0].covered = true;
    viewports
        .record(window, &layout, &current.viewports, current.layout_revision)
        .unwrap();
    assert!(viewports
        .slot_bounds(&browser, window)
        .iter()
        .all(|(_, rect)| *rect == measured));
    layout.viewports[0].rect.width = 0.0;
    viewports
        .record(window, &layout, &current.viewports, current.layout_revision)
        .unwrap();
    assert!(viewports
        .slot_bounds(&browser, window)
        .iter()
        .all(|(_, rect)| *rect == measured));
}

#[test]
fn invalid_layouts_never_partially_replace_measured_geometry() {
    let viewports = Viewports::default();
    let report = report();
    viewports.record(WindowId(1), &report, &assignments(), 4).unwrap();
    let mut invalid = report.clone();
    invalid.viewports[0].rect.width = 500.0;
    invalid.overlays.push(crate::chrome::Overlay {
        rect: Viewport::default(),
        radius: -1.0,
    });
    assert!(viewports.record(WindowId(1), &invalid, &assignments(), 4).is_err());
    assert_eq!(viewports.get(WindowId(1), "1"), report.viewports[0].rect);
    assert!(viewports.can_show(&target()));
}

#[test]
fn stale_layouts_are_ignored_before_validation_or_geometry_changes() {
    let viewports = Viewports::default();
    viewports.record(WindowId(1), &report(), &assignments(), 4).unwrap();
    let mut stale = report();
    stale.revision = 3;
    stale.viewports.clear();
    assert!(!viewports.record(WindowId(1), &stale, &assignments(), 4).unwrap());
    assert_eq!(viewports.get(WindowId(1), "1"), report().viewports[0].rect);
    assert!(!viewports.can_show(&ViewportTarget {
        revision: 5,
        ..target()
    }));
}

#[test]
fn covered_reports_keep_bounds_and_zero_size_reports_hide_surfaces() {
    let viewports = Viewports::default();
    let mut report = report();
    viewports.record(WindowId(1), &report, &assignments(), 4).unwrap();
    let measured = report.viewports[0].rect;
    report.viewports[0].covered = true;
    viewports.record(WindowId(1), &report, &assignments(), 4).unwrap();
    assert_eq!(viewports.get(WindowId(1), "1"), measured);
    assert!(viewports.can_show(&target()));
    report.viewports[0].covered = false;
    report.viewports[0].rect.width = 0.0;
    viewports.record(WindowId(1), &report, &assignments(), 4).unwrap();
    assert_eq!(viewports.get(WindowId(1), "1"), measured);
    assert!(!viewports.can_show(&target()));
    report.viewports[0].rect = measured;
    viewports.record(WindowId(1), &report, &assignments(), 4).unwrap();
    assert!(viewports.can_show(&target()));
}

#[test]
fn geometry_and_readiness_are_local_to_the_window_and_viewport() {
    let viewports = Viewports::default();
    viewports.record(WindowId(1), &report(), &assignments(), 4).unwrap();
    assert_eq!(viewports.get(WindowId(2), "1"), Viewport::default());
    assert_eq!(viewports.get(WindowId(1), "2"), Viewport::default());
    assert!(!viewports.can_show(&ViewportTarget {
        window: WindowId(2),
        ..target()
    }));
    assert!(!viewports.can_show(&ViewportTarget {
        id: "2".into(),
        ..target()
    }));
}

#[test]
fn stale_split_geometry_is_ignored_and_covered_sibling_bounds_are_preserved() {
    let mut browser = Browser::new(4);
    let (left, _) = browser.open_tab("https://left.test", true);
    let (background, _) = browser.open_tab("https://background.test", true);
    browser.select_tab(left, 1).unwrap();
    let (right, _) = browser.open_tab("https://right.test", false);
    let window = browser.main_window();
    browser.split_tab(window, right).unwrap();
    let current = browser.state();
    let mut layout = report();
    layout.revision = current.layout_revision;
    layout.viewports[0].rect.width = 400.0;
    let mut sibling = layout.viewports[0].clone();
    sibling.id = current.viewports[1].id.clone();
    sibling.rect.x = 408.0;
    sibling.covered = true;
    layout.viewports.push(sibling);
    let viewports = Viewports::default();
    viewports
        .record(window, &layout, &current.viewports, current.layout_revision)
        .unwrap();
    let bounds = viewports.slot_bounds(&browser, window);
    assert!(bounds.contains(&(browser.tab(left).unwrap().slot().unwrap(), layout.viewports[0].rect)));
    assert!(bounds.contains(&(browser.tab(right).unwrap().slot().unwrap(), layout.viewports[1].rect)));
    assert!(bounds.contains(&(
        browser.tab(background).unwrap().slot().unwrap(),
        layout.viewports[1].rect
    )));
    browser.focus_pane(window, left).unwrap();
    assert!(viewports.slot_bounds(&browser, window).contains(&(
        browser.tab(background).unwrap().slot().unwrap(),
        layout.viewports[0].rect
    )));
    browser.focus_pane(window, right).unwrap();
    browser.separate_tabs(window, left).unwrap();
    let closed = browser.state();
    assert!(!viewports
        .record(window, &layout, &closed.viewports, closed.layout_revision)
        .unwrap());
    assert!(!viewports.can_show(&browser.slot_viewports()[&browser.tab(right).unwrap().slot().unwrap()]));
    let mut resized = layout.clone();
    resized.revision = closed.layout_revision;
    resized.viewports.remove(0);
    resized.viewports[0].rect.width = 900.0;
    viewports
        .record(window, &resized, &closed.viewports, closed.layout_revision)
        .unwrap();
    assert_eq!(viewports.get(window, &closed.viewports[0].id).width, 900.0);
}

#[test]
fn background_effect_bounds_follow_focus_after_the_initial_pane_is_removed() {
    let mut browser = Browser::new(4);
    let (left, _) = browser.open_tab("https://left.test", true);
    browser.set_fixed(left, true).unwrap();
    let (right, _) = browser.open_tab("https://right.test", false);
    let window = browser.main_window();
    browser.split_tab(window, right).unwrap();
    browser.separate_tabs(window, left).unwrap();
    let current = browser.state();
    let mut layout = report();
    layout.revision = current.layout_revision;
    layout.viewports[0].id.clone_from(&current.focused_viewport);
    let viewports = Viewports::default();
    viewports
        .record(window, &layout, &current.viewports, current.layout_revision)
        .unwrap();
    let bounds = viewports.all_slot_bounds(&browser);
    assert_eq!(
        bounds[&browser.tab(left).unwrap().slot().unwrap()],
        layout.viewports[0].rect
    );
}

#[test]
fn equal_ratio_pair_switch_rejects_old_layout_and_pending_focus() {
    let mut browser = Browser::new(4);
    let (left, _) = browser.open_tab("https://left.test", true);
    let (right, _) = browser.open_tab("https://right.test", false);
    let window = browser.main_window();
    browser.split_tab(window, right).unwrap();
    let (other, _) = browser.open_tab("https://other.test", true);
    let (partner, _) = browser.open_tab("https://partner.test", false);
    browser.split_tab(window, partner).unwrap();
    browser.select_tab(right, 0).unwrap();
    browser.resolve_pane_focus(&[], |_| true);
    let original = browser.state();
    let mut layout = report();
    layout.revision = original.layout_revision;
    layout.viewports[0].id.clone_from(&original.viewports[0].id);
    layout.viewports[0].rect.width = 400.0;
    let mut sibling = layout.viewports[0].clone();
    sibling.id.clone_from(&original.viewports[1].id);
    sibling.rect.x = 408.0;
    layout.viewports.push(sibling);
    let measured = Viewports::default();
    measured
        .record(window, &layout, &original.viewports, original.layout_revision)
        .unwrap();
    let old_focus = browser.focus_pane(window, left).unwrap();
    browser.resolve_pane_focus(&old_focus, |_| false);
    browser.select_tab(partner, 1).unwrap();
    let current = browser.state();
    assert_eq!(current.splits[0].ratio, current.splits[1].ratio);
    assert_eq!(current.layout_revision, original.layout_revision + 1);
    assert!(current
        .viewports
        .iter()
        .all(|pane| original.viewports.iter().all(|old| old.id != pane.id)));
    assert!(!measured
        .record(window, &layout, &current.viewports, current.layout_revision)
        .unwrap());
    assert!(browser.resolve_pane_focus(&[], |_| false).is_empty());
    let focus = browser.resolve_pane_focus(&[], |_| true);
    assert_eq!(
        focus,
        [Effect::FocusPane {
            window,
            slot: browser.tab(partner).unwrap().slot()
        }]
    );
    assert!(!measured.can_show(&browser.slot_viewports()[&browser.tab(other).unwrap().slot().unwrap()]));
    browser.select_tab(left, 1).unwrap();
    let returning = browser.state();
    assert_eq!(returning.layout_revision, current.layout_revision + 1);
    assert!(!measured
        .record(window, &layout, &returning.viewports, returning.layout_revision)
        .unwrap());
}

#[test]
fn new_split_slots_use_measured_window_bounds_until_their_panes_arrive() {
    let mut browser = Browser::new(2);
    let (left, _) = browser.open_tab("https://left.test", true);
    let window = browser.main_window();
    let current = browser.state();
    let mut layout = report();
    layout.revision = current.layout_revision;
    layout.viewports[0].id.clone_from(&current.focused_viewport);
    let viewports = Viewports::default();
    viewports
        .record(window, &layout, &current.viewports, current.layout_revision)
        .unwrap();
    let (right, _) = browser.open_tab("https://right.test", false);
    browser.split_tab(window, right).unwrap();
    let bounds = viewports.all_slot_bounds(&browser);
    for tab in [left, right] {
        let slot = browser.tab(tab).unwrap().slot().unwrap();
        assert_eq!(bounds[&slot], layout.viewports[0].rect);
        assert!(!viewports.can_show(&browser.slot_viewports()[&slot]));
    }
}

#[test]
fn new_window_guesses_skip_unmeasured_normal_panes_and_popups() {
    let viewports = Viewports::default();
    let mut collapsed = report();
    collapsed.viewports[0].rect.width = 0.0;
    viewports.record(WindowId(1), &collapsed, &assignments(), 4).unwrap();
    viewports.open(WindowId(2), WindowKind::Popup, report().viewports[0].rect);
    assert_eq!(viewports.any(), Viewport::default());
    let measured = report().viewports[0].rect;
    viewports.open(WindowId(3), WindowKind::Normal, Viewport::default());
    // Keep the first normal pane collapsed regardless of this map's iteration order.
    {
        let mut cache = viewports.0.write().unwrap();
        let mut normal = cache.values_mut().filter(|pane| pane.kind == WindowKind::Normal);
        normal.next().unwrap();
        normal.next().unwrap().bounds = measured;
    }
    assert_eq!(viewports.any(), measured);
}
