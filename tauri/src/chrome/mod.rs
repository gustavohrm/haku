//! Keeps the chrome layered over page content and input routed correctly.
//!
//! The chrome webview covers the whole window and sits above every content
//! webview, which is what allows rounded viewport corners and menus that
//! overlap the page. The cost is that it would swallow every click, so the
//! interface reports which rectangles it actually occupies and this module
//! turns them into a native input mask.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::browser::ViewportAssignment;
use crate::error::{HakuError, Result};
use crate::platform::{self, RoundedRect};
use crate::webview::Viewport;

/// A region drawn above the page, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct Overlay {
    pub rect: Viewport,
    /// Corner radius, read from the element's own style. A rounded overlay
    /// needs a rounded region, or its corners would show chrome over the page.
    #[specta(type = specta_typescript::Number)]
    pub radius: f64,
}

/// Geometry for one assigned pane, in chrome CSS pixels.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct PaneLayout {
    pub id: String,
    pub rect: Viewport,
    pub covered: bool,
    #[specta(type = specta_typescript::Number)]
    pub radius: f64,
}

/// A complete measurement of the assignments at one browser revision.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
pub struct Layout {
    #[specta(type = specta_typescript::Number)]
    pub revision: u64,
    pub viewports: Vec<PaneLayout>,
    pub overlays: Vec<Overlay>,
}

impl Layout {
    /// Validates the whole report before any geometry or native state changes.
    ///
    /// # Errors
    /// Returns [`HakuError::Unsupported`] for an invalid or incomplete report.
    pub fn validate(&self, assignments: &[ViewportAssignment]) -> Result<()> {
        let invalid = |reason: &str| HakuError::Unsupported(format!("invalid layout: {reason}"));
        let mut ids = std::collections::HashSet::new();
        for pane in &self.viewports {
            if !ids.insert(&pane.id) {
                return Err(invalid("duplicate viewport id"));
            }
            if !assignments.iter().any(|assigned| assigned.id == pane.id) {
                return Err(invalid("unknown viewport id"));
            }
        }
        if ids.len() != assignments.len() {
            return Err(invalid("missing viewport assignment"));
        }
        for (rect, radius) in self
            .viewports
            .iter()
            .map(|pane| (pane.rect, pane.radius))
            .chain(self.overlays.iter().map(|overlay| (overlay.rect, overlay.radius)))
        {
            if ![rect.x, rect.y, rect.width, rect.height, radius]
                .iter()
                .all(|value| value.is_finite())
                || rect.width < 0.0
                || rect.height < 0.0
                || radius < 0.0
            {
                return Err(invalid("non-finite geometry or negative size/radius"));
            }
        }
        for (index, pane) in self.viewports.iter().enumerate() {
            let a = pane.rect;
            for other in &self.viewports[index + 1..] {
                let b = other.rect;
                if a.width > 0.0
                    && a.height > 0.0
                    && b.width > 0.0
                    && b.height > 0.0
                    && a.x < b.x + b.width
                    && b.x < a.x + a.width
                    && a.y < b.y + b.height
                    && b.y < a.y + a.height
                {
                    return Err(invalid("intersecting viewport rectangles"));
                }
            }
        }
        Ok(())
    }
}

/// Converts a logical rectangle and corner radius to exact device pixels.
///
/// The mask is applied with native APIs that work in device pixels, so the
/// scale factor is resolved once here rather than inside platform code. Edges
/// stay fractional until the shape is cut into pixels, so its curve lands
/// where CSS draws it.
fn to_physical(viewport: Viewport, radius: f64, scale: f64) -> RoundedRect {
    RoundedRect {
        left: viewport.x * scale,
        top: viewport.y * scale,
        right: (viewport.x + viewport.width) * scale,
        bottom: (viewport.y + viewport.height) * scale,
        radius: radius.max(0.0) * scale,
    }
}

/// A viewport small enough to be invisible should not punch a hole at all.
///
/// During layout the interface can briefly report a collapsed rectangle; masking
/// on it would flash the page through a sliver of the interface.
fn is_visible(viewport: Viewport) -> bool {
    viewport.width >= 1.0 && viewport.height >= 1.0
}

/// The uncovered pane holes and overlays, in device pixels.
pub fn mask_rects(layout: &Layout, scale: f64) -> (Vec<RoundedRect>, Vec<RoundedRect>) {
    let viewports = layout
        .viewports
        .iter()
        .filter(|pane| !pane.covered && is_visible(pane.rect))
        .map(|pane| to_physical(pane.rect, pane.radius, scale))
        .collect();
    let overlays = layout
        .overlays
        .iter()
        .filter(|overlay| is_visible(overlay.rect))
        .map(|overlay| to_physical(overlay.rect, overlay.radius, scale))
        .collect();
    (viewports, overlays)
}

/// Applies a reported layout: raises the chrome and rebuilds its input mask.
///
/// # Errors
/// Propagates platform failures, including [`crate::error::HakuError::Unsupported`]
/// on targets without a native implementation.
pub fn apply_layout<R: tauri::Runtime>(chrome: &tauri::Webview<R>, layout: &Layout, scale: f64) -> Result<()> {
    let (viewports, overlays) = mask_rects(layout, scale);
    platform::raise_chrome(chrome)?;
    platform::set_input_mask(chrome, &viewports, &overlays)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport(x: f64, y: f64, width: f64, height: f64) -> Viewport {
        Viewport { x, y, width, height }
    }

    fn square(x: f64, y: f64, width: f64, height: f64) -> Overlay {
        Overlay {
            rect: viewport(x, y, width, height),
            radius: 0.0,
        }
    }

    #[test]
    fn logical_pixels_scale_to_exact_device_pixels() {
        let shape = to_physical(viewport(10.0, 20.0, 33.0, 50.0), 4.0, 1.25);

        assert_eq!(
            shape,
            RoundedRect {
                left: 12.5,
                top: 25.0,
                right: 53.75,
                bottom: 87.5,
                radius: 5.0
            }
        );
    }

    #[test]
    fn a_layout_without_a_viewport_masks_nothing_so_the_chrome_stays_solid() {
        let layout = Layout {
            revision: 0,
            viewports: Vec::new(),
            overlays: Vec::new(),
        };
        let (viewport, overlays) = mask_rects(&layout, 1.0);

        assert!(viewport.is_empty());
        assert!(overlays.is_empty());
    }

    #[test]
    fn a_collapsed_viewport_is_ignored_so_no_sliver_of_page_shows_through() {
        let layout = Layout {
            revision: 0,
            viewports: vec![PaneLayout {
                id: "1".into(),
                rect: viewport(0.0, 0.0, 0.0, 800.0),
                covered: false,
                radius: 0.0,
            }],
            overlays: Vec::new(),
        };
        let (viewport, _) = mask_rects(&layout, 1.0);

        assert!(viewport.is_empty());
    }

    #[test]
    fn overlays_are_carried_through_so_open_menus_stay_clickable() {
        let layout = Layout {
            revision: 0,
            viewports: vec![PaneLayout {
                id: "1".into(),
                rect: viewport(0.0, 40.0, 1000.0, 800.0),
                covered: false,
                radius: 0.0,
            }],
            overlays: vec![square(100.0, 100.0, 200.0, 300.0)],
        };
        let (_, overlays) = mask_rects(&layout, 2.0);

        assert_eq!(
            overlays,
            vec![RoundedRect {
                left: 200.0,
                top: 200.0,
                right: 600.0,
                bottom: 800.0,
                radius: 0.0
            }]
        );
    }

    #[test]
    fn an_overlay_keeps_its_corner_radius_scaled_to_the_display() {
        let layout = Layout {
            revision: 0,
            viewports: vec![PaneLayout {
                id: "1".into(),
                rect: viewport(0.0, 40.0, 1000.0, 800.0),
                covered: false,
                radius: 0.0,
            }],
            overlays: vec![Overlay {
                rect: viewport(10.0, 10.0, 100.0, 100.0),
                radius: 8.0,
            }],
        };
        let (_, overlays) = mask_rects(&layout, 1.5);

        assert_eq!(overlays[0].radius, 12.0);
    }

    #[test]
    fn the_corner_radius_scales_with_the_display() {
        let layout = Layout {
            revision: 0,
            viewports: vec![PaneLayout {
                id: "1".into(),
                rect: viewport(0.0, 0.0, 100.0, 100.0),
                covered: false,
                radius: 0.0,
            }],
            overlays: Vec::new(),
        };
        let mut layout = layout;
        layout.viewports[0].radius = 14.0;
        let (viewports, _) = mask_rects(&layout, 1.5);

        assert_eq!(viewports[0].radius, 21.0);
    }

    #[test]
    fn a_negative_radius_is_treated_as_square_corners() {
        let layout = Layout {
            revision: 0,
            viewports: vec![PaneLayout {
                id: "1".into(),
                rect: viewport(0.0, 0.0, 100.0, 100.0),
                covered: false,
                radius: 0.0,
            }],
            overlays: Vec::new(),
        };
        let mut layout = layout;
        layout.viewports[0].radius = -5.0;
        let (viewports, _) = mask_rects(&layout, 1.0);

        assert_eq!(viewports[0].radius, 0.0);
    }

    #[test]
    fn collapsed_overlays_are_dropped_along_with_collapsed_viewports() {
        let layout = Layout {
            revision: 0,
            viewports: vec![PaneLayout {
                id: "1".into(),
                rect: viewport(0.0, 40.0, 1000.0, 800.0),
                covered: false,
                radius: 0.0,
            }],
            overlays: vec![square(10.0, 10.0, 0.0, 0.0)],
        };
        let (_, overlays) = mask_rects(&layout, 1.0);

        assert!(overlays.is_empty());
    }
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod layout_tests;
