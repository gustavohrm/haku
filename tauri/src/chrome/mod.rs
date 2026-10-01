//! Keeps the chrome layered over page content and input routed correctly.
//!
//! The chrome webview covers the whole window and sits above every content
//! webview, which is what allows rounded viewport corners and menus that
//! overlap the page. The cost is that it would swallow every click, so the
//! interface reports which rectangles it actually occupies and this module
//! turns them into a native input mask.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::Result;
use crate::platform::{self, PhysicalRect};
use crate::webview::Viewport;

/// What the interface currently occupies, in logical pixels.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// Where page content is shown. `None` means the chrome covers everything,
    /// which is the case for an internal page or a full-screen overlay.
    pub viewport: Option<Viewport>,
    /// Regions drawn above the page right now, such as an open menu. These are
    /// added back to the chrome's input area so they remain clickable.
    #[serde(default)]
    pub overlays: Vec<Viewport>,
    /// Corner radius of the viewport, in logical pixels.
    ///
    /// The page is a native view and cannot be clipped by CSS, so the hole cut
    /// in the chrome is rounded instead. The interface reports what its own
    /// stylesheet uses, keeping CSS the single source of that number.
    #[serde(default)]
    #[specta(type = specta_typescript::Number)]
    pub radius: f64,
}

/// Converts a logical rectangle to physical pixels.
///
/// The mask is applied with native APIs that work in device pixels, so the
/// scale factor is resolved once here rather than inside platform code.
pub fn to_physical(viewport: Viewport, scale: f64) -> PhysicalRect {
    PhysicalRect {
        x: (viewport.x * scale).round() as i32,
        y: (viewport.y * scale).round() as i32,
        width: (viewport.width * scale).round() as i32,
        height: (viewport.height * scale).round() as i32,
    }
}

/// A viewport small enough to be invisible should not punch a hole at all.
///
/// During layout the interface can briefly report a collapsed rectangle; masking
/// on it would flash the page through a sliver of the interface.
fn is_visible(viewport: Viewport) -> bool {
    viewport.width >= 1.0 && viewport.height >= 1.0
}

/// The viewport hole, its corner radius, and the overlays, in device pixels.
pub fn mask_rects(layout: &Layout, scale: f64) -> (Option<PhysicalRect>, i32, Vec<PhysicalRect>) {
    let viewport = layout.viewport.filter(|viewport| is_visible(*viewport)).map(|v| to_physical(v, scale));
    let overlays = layout
        .overlays
        .iter()
        .copied()
        .filter(|overlay| is_visible(*overlay))
        .map(|overlay| to_physical(overlay, scale))
        .collect();

    let radius = (layout.radius.max(0.0) * scale).round() as i32;
    (viewport, radius, overlays)
}

/// Applies a reported layout: raises the chrome and rebuilds its input mask.
///
/// # Errors
/// Propagates platform failures, including [`crate::error::HakuError::Unsupported`]
/// on targets without a native implementation.
pub fn apply_layout<R: tauri::Runtime>(
    chrome: &tauri::Webview<R>,
    layout: &Layout,
    scale: f64,
) -> Result<()> {
    let (viewport, radius, overlays) = mask_rects(layout, scale);
    platform::raise_chrome(chrome)?;
    platform::set_input_mask(chrome, viewport, radius, &overlays)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport(x: f64, y: f64, width: f64, height: f64) -> Viewport {
        Viewport { x, y, width, height }
    }

    #[test]
    fn logical_pixels_scale_to_device_pixels() {
        let rect = to_physical(viewport(10.0, 20.0, 100.0, 50.0), 1.5);

        assert_eq!(rect, PhysicalRect { x: 15, y: 30, width: 150, height: 75 });
    }

    #[test]
    fn fractional_device_pixels_round_rather_than_truncate() {
        let rect = to_physical(viewport(10.0, 10.0, 33.0, 33.0), 1.25);

        assert_eq!(rect.width, 41);
        assert_eq!(rect.height, 41);
    }

    #[test]
    fn a_layout_without_a_viewport_masks_nothing_so_the_chrome_stays_solid() {
        let layout = Layout { viewport: None, overlays: Vec::new(), radius: 0.0 };
        let (viewport, _, overlays) = mask_rects(&layout, 1.0);

        assert!(viewport.is_none());
        assert!(overlays.is_empty());
    }

    #[test]
    fn a_collapsed_viewport_is_ignored_so_no_sliver_of_page_shows_through() {
        let layout = Layout { viewport: Some(viewport(0.0, 0.0, 0.0, 800.0)), overlays: Vec::new(), radius: 0.0 };
        let (viewport, _, _) = mask_rects(&layout, 1.0);

        assert!(viewport.is_none());
    }

    #[test]
    fn overlays_are_carried_through_so_open_menus_stay_clickable() {
        let layout = Layout {
            viewport: Some(viewport(0.0, 40.0, 1000.0, 800.0)),
            overlays: vec![viewport(100.0, 100.0, 200.0, 300.0)],
            radius: 0.0,
        };
        let (_, _, overlays) = mask_rects(&layout, 2.0);

        assert_eq!(overlays, vec![PhysicalRect { x: 200, y: 200, width: 400, height: 600 }]);
    }

    #[test]
    fn the_corner_radius_scales_with_the_display() {
        let layout =
            Layout { viewport: Some(viewport(0.0, 0.0, 100.0, 100.0)), overlays: Vec::new(), radius: 14.0 };
        let (_, radius, _) = mask_rects(&layout, 1.5);

        assert_eq!(radius, 21);
    }

    #[test]
    fn a_negative_radius_is_treated_as_square_corners() {
        let layout =
            Layout { viewport: Some(viewport(0.0, 0.0, 100.0, 100.0)), overlays: Vec::new(), radius: -5.0 };
        let (_, radius, _) = mask_rects(&layout, 1.0);

        assert_eq!(radius, 0);
    }

    #[test]
    fn collapsed_overlays_are_dropped_along_with_collapsed_viewports() {
        let layout = Layout {
            viewport: Some(viewport(0.0, 40.0, 1000.0, 800.0)),
            overlays: vec![viewport(10.0, 10.0, 0.0, 0.0)],
            radius: 0.0,
        };
        let (_, _, overlays) = mask_rects(&layout, 1.0);

        assert!(overlays.is_empty());
    }
}
