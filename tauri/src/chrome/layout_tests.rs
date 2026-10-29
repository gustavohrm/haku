use super::*;
use crate::browser::ViewportAssignment;
use crate::model::TabId;

fn pane(id: &str, rect: Viewport) -> PaneLayout {
    PaneLayout {
        id: id.into(),
        rect,
        covered: false,
        radius: 8.0,
    }
}

fn rect(x: f64, width: f64) -> Viewport {
    Viewport {
        x,
        y: 10.0,
        width,
        height: 100.0,
    }
}

fn assignments() -> Vec<ViewportAssignment> {
    vec![ViewportAssignment {
        id: "1".into(),
        tab: TabId(1),
    }]
}

fn layout() -> Layout {
    Layout {
        revision: 1,
        viewports: vec![pane("1", rect(0.0, 100.0))],
        overlays: Vec::new(),
    }
}

#[test]
fn duplicate_viewport_ids_are_rejected() {
    let mut report = layout();
    report.viewports.push(report.viewports[0].clone());
    assert!(report.validate(&assignments()).is_err());
}

#[test]
fn unknown_viewport_ids_are_rejected() {
    let mut report = layout();
    report.viewports[0].id = "foreign".into();
    assert!(report.validate(&assignments()).is_err());
}

#[test]
fn missing_assignments_are_rejected() {
    let mut report = layout();
    report.viewports.clear();
    assert!(report.validate(&assignments()).is_err());
}

#[test]
fn non_finite_geometry_and_negative_sizes_or_radii_are_rejected() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        for field in ["x", "y", "width", "height", "radius"] {
            if value == -1.0 && matches!(field, "x" | "y") {
                continue;
            }
            for overlay in [false, true] {
                let mut report = layout();
                let mut region = Overlay {
                    rect: rect(0.0, 10.0),
                    radius: 0.0,
                };
                match field {
                    "x" => region.rect.x = value,
                    "y" => region.rect.y = value,
                    "width" => region.rect.width = value,
                    "height" => region.rect.height = value,
                    _ => region.radius = value,
                }
                if overlay {
                    report.overlays.push(region);
                } else {
                    report.viewports[0].rect = region.rect;
                    report.viewports[0].radius = region.radius;
                }
                assert!(
                    report.validate(&assignments()).is_err(),
                    "{field}={value}, overlay={overlay}"
                );
            }
        }
    }
}

#[test]
fn positive_area_intersections_are_rejected_but_touching_edges_are_valid() {
    let mut assigned = assignments();
    assigned.push(ViewportAssignment {
        id: "2".into(),
        tab: TabId(2),
    });
    let mut report = layout();
    report.viewports.push(pane("2", rect(99.0, 100.0)));
    assert!(report.validate(&assigned).is_err());
    report.viewports[1].rect.x = 100.0;
    assert!(report.validate(&assigned).is_ok());
    report.viewports[1].rect = rect(50.0, 0.0);
    assert!(report.validate(&assigned).is_ok());
}

#[test]
fn covered_and_zero_size_panes_make_no_holes() {
    let mut report = layout();
    report.viewports[0].covered = true;
    assert!(mask_rects(&report, 1.0).0.is_empty());
    report.viewports[0].covered = false;
    report.viewports[0].rect.width = 0.0;
    assert!(mask_rects(&report, 1.0).0.is_empty());
}
