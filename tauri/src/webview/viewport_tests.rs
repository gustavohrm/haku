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
